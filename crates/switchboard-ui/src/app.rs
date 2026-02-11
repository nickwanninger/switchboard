use eframe::{egui, App, Frame};
use std::sync::mpsc::{Receiver, channel, Sender};
use switchboard_core::{
    CommandExecutor, CommandStore, ExecutionUpdate,
    save_command,
};
use uuid::Uuid;
use std::collections::HashMap;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Selection {
    Command(Uuid),
    Execution(Uuid),
    Workflow(Uuid),
}

pub struct ExecutionState {
    pub id: Uuid,
    pub _command_id: Uuid,
    pub command_name: String,
    pub output_buffer: String,
    pub is_running: bool,
    pub exit_code: Option<i32>,
    pub kill_tx: Option<Sender<()>>,
    pub working_directory: Option<String>,
    pub is_local: bool,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub output_loaded: bool,
    pub is_from_history: bool,
}

struct PendingExecution {
    cmd_id: Option<Uuid>, // Single command ID if running a command
    workflow_id: Option<Uuid>, // Workflow ID if running a workflow
    
    // For single command: the default vars
    // For workflow: the initial overrides (from workflow)
    initial_vars: std::collections::HashMap<String, String>,
    
    // The variables that require user input.
    // We store them as EnvVars so we can edit the value.
    vars_to_ask: Vec<switchboard_core::models::EnvVar>,
}

#[derive(Clone, Default)]
struct CommandEditState {
    name: String,
    description: String,
    host: String,
    user: String,
    working_directory: String,
    script: String,
    is_local: bool,
    background: bool,
    env_vars: Vec<switchboard_core::models::EnvVar>,
}

impl CommandEditState {
    fn from_command(cmd: &switchboard_core::models::Command) -> Self {
        Self {
            name: cmd.name.clone(),
            description: cmd.description.clone().unwrap_or_default(),
            host: cmd.host.clone().unwrap_or_default(),
            user: cmd.user.clone().unwrap_or_default(),
            working_directory: cmd.working_directory.clone().unwrap_or_default(),
            script: cmd.script.clone(),
            is_local: cmd.host.is_none(),
            background: cmd.background,
            env_vars: cmd.env_vars.clone(),
        }
    }
    
    fn apply_to_command(&self, cmd: &mut switchboard_core::models::Command) {
        cmd.name = self.name.clone();
        cmd.description = if self.description.is_empty() { None } else { Some(self.description.clone()) };
        
        if self.is_local {
            cmd.host = None;
            cmd.user = None;
        } else {
            // We use Some even for empty strings so that 'is_local' (which is derived from host.is_none())
            // doesn't accidentally become true just because the host field was cleared.
            cmd.host = Some(self.host.clone());
            cmd.user = Some(self.user.clone());
        }
        
        cmd.working_directory = if self.working_directory.is_empty() { None } else { Some(self.working_directory.clone()) };
        cmd.script = self.script.clone();
        cmd.background = self.background;
        cmd.env_vars = self.env_vars.clone();
    }
}

#[derive(Clone, Default)]
struct WorkflowEditState {
    name: String,
    description: String,
    commands: Vec<Uuid>,
    env_vars: Vec<switchboard_core::models::EnvVar>,
}

impl WorkflowEditState {
    fn from_workflow(wf: &switchboard_core::models::Workflow) -> Self {
        Self {
            name: wf.name.clone(),
            description: wf.description.clone().unwrap_or_default(),
            commands: wf.commands.clone(),
            env_vars: wf.env_vars.clone(),
        }
    }
    
    fn apply_to_workflow(&self, wf: &mut switchboard_core::models::Workflow) {
        wf.name = self.name.clone();
        wf.description = if self.description.is_empty() { None } else { Some(self.description.clone()) };
        wf.commands = self.commands.clone();
        wf.env_vars = self.env_vars.clone();
    }
}

pub struct ActiveWorkflow {
    pub workflow_id: Uuid,
    pub current_step_index: usize,
    pub current_execution_id: Option<Uuid>,
    pub resolved_env: std::collections::HashMap<String, String>,
}

pub struct SwitchboardApp {
    store: CommandStore,
    executor: Box<dyn CommandExecutor>,
    
    // Selection State
    active_selection: Option<Selection>,
    navigation_history: Vec<Selection>,
    
    // UI State
    sidebar_width: f32,
    show_delete_confirmation: bool,

    // Editing State
    edited_command: Option<CommandEditState>,
    edited_workflow: Option<WorkflowEditState>,
    
    // Prompt State
    pending_execution: Option<PendingExecution>,

    // Execution State
    active_workflow: Option<ActiveWorkflow>,
    executions: Vec<ExecutionState>,
    // We send (ExecutionID, Update) to identify which run the update belongs to
    execution_tx: Sender<(Uuid, ExecutionUpdate)>,
    execution_rx: Receiver<(Uuid, ExecutionUpdate)>,
}

impl SwitchboardApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Customize visuals
        let mut visuals = egui::Visuals::dark();
        visuals.selection.bg_fill = egui::Color32::from_rgb(46, 134, 91); 
        visuals.selection.stroke.color = egui::Color32::WHITE;
        cc.egui_ctx.set_visuals(visuals);

        let store = CommandStore::new();

        // Pre-load all execution history from the store
        let all_commands = store.list_commands();
        let mut executions: Vec<ExecutionState> = all_commands
            .iter()
            .flat_map(|cmd| {
                store.get_execution_history(&cmd.id).into_iter().map(|item| ExecutionState {
                    id: item.id,
                    _command_id: item.command_id,
                    command_name: cmd.name.clone(),
                    output_buffer: String::from("(Click to load logs)"),
                    is_running: false,
                    exit_code: item.exit_code,
                    kill_tx: None,
                    working_directory: None,
                    is_local: false,
                    started_at: item.started_at,
                    output_loaded: false,
                    is_from_history: true,
                })
            })
            .collect();
        executions.sort_by(|a, b| a.started_at.cmp(&b.started_at));

        // Execution channel
        let (exec_tx, exec_rx) = channel();

        use switchboard_core::Executor;

        Self {
            store,
            executor: Box::new(Executor),
            active_selection: None,
            navigation_history: Vec::new(),
            sidebar_width: 250.0,
            show_delete_confirmation: false,
            edited_command: None,
            edited_workflow: None,
            pending_execution: None,
            active_workflow: None,
            executions,
            execution_tx: exec_tx,
            execution_rx: exec_rx,
        }
    }

    fn navigate_to(&mut self, selection: Selection) {
        if let Some(current) = self.active_selection {
            if current != selection {
                self.save_current_command();
                self.save_current_workflow();
                self.navigation_history.push(current);
                self.active_selection = Some(selection);
            }
        } else {
            self.active_selection = Some(selection);
        }
    }

    fn navigate_back(&mut self) {
        if let Some(prev) = self.navigation_history.pop() {
            self.save_current_command();
            self.save_current_workflow();
            self.active_selection = Some(prev);
            
            // Re-initialize edit state if needed based on selection type
             match prev {
                Selection::Command(id) => {
                    if let Some(cmd) = self.store.get_command(&id) {
                        self.edited_command = Some(CommandEditState::from_command(&cmd));
                        self.edited_workflow = None;
                    }
                },
                Selection::Workflow(id) => {
                    if let Some(wf) = self.store.get_workflow(&id) {
                        self.edited_workflow = Some(WorkflowEditState::from_workflow(&wf));
                        self.edited_command = None;
                    }
                },
                _ => {}
            }
        } else {
             // If history is empty, maybe go to "home" (None)?
             if self.active_selection.is_some() {
                 self.save_current_command();
                 self.save_current_workflow();
                 self.active_selection = None;
                 self.edited_command = None;
                 self.edited_workflow = None;
             }
        }
    }

    fn trigger_workflow_execution(&mut self, workflow_id: Uuid) {
         if let Some(wf) = self.store.get_workflow(&workflow_id) {
            if wf.commands.is_empty() {
                return;
            }
            

            
            // Trigger first command
            if let Some(first_cmd_id) = wf.commands.first() {
                // We pass the resolved env to the command execution
                // But wait, trigger_command_execution takes just ID.
                // We need to modify trigger_command_execution or handle it here.
                // actually, we should start the workflow AFTER prompt.
                // So...
                
                // 1. Gather all vars from all commands
                use std::collections::HashMap;
                let mut vars_to_ask = Vec::new();
                let mut resolved_env = HashMap::new();
                
                // Add workflow overrides (higher priority than defaults, but user input is highest)
                for v in &wf.env_vars {
                    resolved_env.insert(v.key.clone(), v.value.clone());
                    if v.ask_user {
                         // Check if already present?
                         if !vars_to_ask.iter().any(|existing: &switchboard_core::models::EnvVar| existing.key == v.key) {
                             vars_to_ask.push(v.clone());
                         }
                    }
                }
                
                for cmd_id in &wf.commands {
                    if let Some(cmd) = self.store.get_command(cmd_id) {
                         for v in &cmd.env_vars {
                             // Only add if not overridden by workflow
                             if !resolved_env.contains_key(&v.key) {
                                 resolved_env.insert(v.key.clone(), v.value.clone());
                             }
                             
                             if v.ask_user {
                                 // Check if overridden?
                                 // If workflow defines it and says ask=false, do we ask?
                                 // The logic: Workflow overrides Command.
                                 // If Workflow has "KEY=VAL", then Command's "KEY=DEFAULT" is ignored.
                                 // If Workflow has "KEY=VAL (ask=false)", and Command has "KEY=DEFAULT (ask=true)", strict override says we don't ask.
                                 // But usually we want to respect "ask" if ANYONE asks.
                                 // However, strict override is simpler.
                                 // Let's assume:
                                 // Effective Var = Workflow Var if present, else Command Var.
                                 // If Effective Var says Ask, we Ask.
                                 
                                 let effective_ask = if let Some(wf_var) = wf.env_vars.iter().find(|ev| ev.key == v.key) {
                                     wf_var.ask_user
                                 } else {
                                     v.ask_user
                                 };
                                 
                                 if effective_ask {
                                     if !vars_to_ask.iter().any(|existing: &switchboard_core::models::EnvVar| existing.key == v.key) {
                                         // Use the resolved value as default
                                         let val = resolved_env.get(&v.key).cloned().unwrap_or_default();
                                         vars_to_ask.push(switchboard_core::models::EnvVar {
                                             key: v.key.clone(),
                                             value: val,
                                             ask_user: true
                                         });
                                     }
                                 }
                             }
                        }
                    }
                }
                
                if !vars_to_ask.is_empty() {
                    self.pending_execution = Some(PendingExecution {
                        cmd_id: None,
                        workflow_id: Some(workflow_id),
                        initial_vars: resolved_env,
                        vars_to_ask,
                    });
                } else {
                    // Start immediately
                    self.active_workflow = Some(ActiveWorkflow {
                        workflow_id,
                        current_step_index: 0,
                        current_execution_id: None,
                        resolved_env: resolved_env.clone(),
                    });
                    self.perform_execution(*first_cmd_id, None);
                }
            }
         }
    }
    
    // Add imports
    

     fn check_workflow_progress(&mut self, finished_exec_id: Uuid, exit_code: i32) {
        if let Some(active_wf) = &mut self.active_workflow {
            // Check if the finished execution matches our current step
             if active_wf.current_execution_id == Some(finished_exec_id) {
                 if exit_code == 0 {
                     // Success, move to next step
                     if let Some(wf) = self.store.get_workflow(&active_wf.workflow_id) {
                         let next_idx = active_wf.current_step_index + 1;
                         if next_idx < wf.commands.len() {
                             active_wf.current_step_index = next_idx;
                             let next_cmd_id = wf.commands[next_idx];
                             self.perform_execution(next_cmd_id, None); 
                         } else {
                             // Workflow finished
                             self.active_workflow = None;
                         }
                     }
                 } else {
                     // Failure, stop workflow
                     self.active_workflow = None;
                 }
             }
        }
    }

    fn trigger_command_execution(&mut self, cmd_id: Uuid) {
         // Save first
        if let Some(Selection::Command(active_id)) = self.active_selection {
            if active_id == cmd_id {
                self.save_current_command();
            }
        }
        
        // If we are in a workflow, we don't prompt (already done).
        // BUT wait, trigger_command_execution is called BY trigger_workflow_execution (in my old code).
        // I changed trigger_workflow_execution to call `perform_execution`.
        // So trigger_command_execution is ONLY for manual single command runs now.
        
        if let Some(cmd) = self.store.get_command(&cmd_id) {
            let mut vars_to_ask = Vec::new();
            let mut initial_vars = HashMap::new();
            
            for v in &cmd.env_vars {
                initial_vars.insert(v.key.clone(), v.value.clone());
                if v.ask_user {
                    vars_to_ask.push(v.clone());
                }
            }
            
            if !vars_to_ask.is_empty() {
                self.pending_execution = Some(PendingExecution {
                    cmd_id: Some(cmd_id),
                    workflow_id: None,
                    initial_vars,
                    vars_to_ask,
                });
            } else {
                // Determine env map from command only
                self.perform_execution(cmd_id, None);
            }
        }
    }
    
    fn perform_execution(&mut self, cmd_id: Uuid, explicit_env: Option<std::collections::HashMap<String, String>>) {
        use switchboard_core::{Host, AuthMethod};
        use std::collections::HashMap;

        // Fetch command to run
        if let Some(cmd) = self.store.get_command(&cmd_id) {
             
             // Detect current user for "localhost" test
             let default_user = std::env::var("USER").unwrap_or_else(|_| "root".into());
             
             let hostname = cmd.host.clone().unwrap_or_else(|| "localhost".to_string());
             let username = cmd.user.clone().unwrap_or(default_user);
             let name = if cmd.host.is_some() { "Remote".into() } else { "local".into() };

             let dummy_host = Host {
                id: Uuid::new_v4(),
                name,
                hostname,
                port: 22,
                username,
                auth: AuthMethod::Agent,
            };
            
            let exec_id = Uuid::new_v4();
            let tx = self.execution_tx.clone();
            
            // Create kill channel
            let (kill_tx, kill_rx) = channel();
            
            let cb = Box::new(move |update| {
                let _ = tx.send((exec_id, update));
            });
            
            // Create State
            let state = ExecutionState {
                id: exec_id,
                _command_id: cmd_id,
                command_name: cmd.name.clone(),
                output_buffer: String::new(),
                is_running: true,
                exit_code: None,
                kill_tx: Some(kill_tx),
                working_directory: cmd.working_directory.clone(),
                is_local: cmd.host.is_none(),
                started_at: chrono::Utc::now(),
                output_loaded: true,
                is_from_history: false,
            };
            self.executions.push(state);
            
            // Switch view
            self.navigate_to(Selection::Execution(exec_id));
            
            // Update active workflow if applicable
            let mut execution_env_vars = HashMap::new();
            
            // 1. Command Defaults
            for v in &cmd.env_vars {
                execution_env_vars.insert(v.key.clone(), v.value.clone());
            }

             if let Some(active_wf) = &mut self.active_workflow {
                 active_wf.current_execution_id = Some(exec_id);
                 // 2. Workflow Overrides / Context
                 for (k, v) in &active_wf.resolved_env {
                     execution_env_vars.insert(k.clone(), v.clone());
                 }
            } else if let Some(overrides) = explicit_env {
                // 3. Explicit Overrides (from Prompt)
                for (k, v) in overrides {
                    execution_env_vars.insert(k, v);
                }
            }
            
            // Run
            if let Err(e) = self.executor.execute(exec_id, &cmd, &dummy_host, execution_env_vars, cb, kill_rx) {
                 eprintln!("Failed to start execution: {}", e);
            }
        }
    }

    fn save_current_command(&mut self) {
        if let Some(Selection::Command(cmd_id)) = self.active_selection {
            if let Some(mut cmd) = self.store.get_command(&cmd_id) {
                if let Some(edit_state) = &self.edited_command {
                    edit_state.apply_to_command(&mut cmd);
                    save_command(&self.store, &cmd);
                }
            }
        }
    }
    
    fn save_current_workflow(&mut self) {
        if let Some(Selection::Workflow(wf_id)) = self.active_selection {
            if let Some(mut wf) = self.store.get_workflow(&wf_id) {
                if let Some(edit_state) = &self.edited_workflow {
                    edit_state.apply_to_workflow(&mut wf);
                    self.store.add_workflow(wf); // add_workflow acts as upsert
                }
            }
        }
    }

    fn create_new_command(&mut self) {
        let id = Uuid::new_v4();
        let cmd = switchboard_core::models::Command {
            id,
            name: "New Command".to_string(),
            description: None,
            script: "".to_string(),
            working_directory: None,
            env_vars: Vec::new(),
            host: None,
            user: std::env::var("USER").ok(),
            target_hosts: Vec::new(),
            created_at: chrono::Utc::now(),
            background: false,
            source_path: None,
        };

        save_command(&self.store, &cmd);
        self.navigate_to(Selection::Command(id));
        self.edited_command = Some(CommandEditState::from_command(&cmd));
    }
    
    fn create_new_workflow(&mut self) {
        let id = Uuid::new_v4();
        let wf = switchboard_core::models::Workflow {
            id,
            name: "New Workflow".to_string(),
            description: None,
            commands: Vec::new(),
            env_vars: Vec::new(),
            created_at: chrono::Utc::now(),
        };

        self.store.add_workflow(wf.clone());
        self.navigate_to(Selection::Workflow(id));
        self.edited_workflow = Some(WorkflowEditState::from_workflow(&wf));
    }
}

impl App for SwitchboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Export Store JSON...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("JSON", &["json"])
                            .save_file() 
                        {
                            match self.store.export_json() {
                                Ok(json) => {
                                    if let Err(e) = std::fs::write(&path, json) {
                                        eprintln!("Failed to write export file: {}", e);
                                    }
                                }
                                Err(e) => eprintln!("Failed to export store: {}", e),
                            }
                        }
                        ui.close();
                    }

                    if ui.button("Import Store JSON...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("JSON", &["json"])
                            .pick_file() 
                        {
                            match std::fs::read_to_string(&path) {
                                Ok(json) => {
                                    if let Err(e) = self.store.import_json(&json) {
                                         eprintln!("Failed to import store: {}", e);
                                    } else {
                                        // Reset selection as data has changed
                                        self.active_selection = None;
                                        self.edited_command = None;
                                        self.edited_workflow = None;
                                        self.active_workflow = None;
                                        // TODO: Maybe reload or refresh specific UI parts if needed
                                    }
                                }
                                Err(e) => eprintln!("Failed to read import file: {}", e),
                            }
                        }
                        ui.close();
                    }

                    if ui.button("Snapshot State").clicked() {
                        match self.store.snapshot_state() {
                            Ok(hash) => println!("Snapshot created: {}", hash),
                            Err(e) => eprintln!("Failed to create snapshot: {}", e),
                        }
                        ui.close();
                    }
                });
            });
        });

        // Global Navigation Shortcuts
        if ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Extra1)) {
            self.navigate_back();
        }

        // No more file system events since we're using a database

        // Delete confirmation modal
        if self.show_delete_confirmation {
            egui::Window::new("⚠️ Confirm Delete")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new("⚠️ WARNING: This action cannot be undone!")
                                .size(16.0)
                                .color(egui::Color32::from_rgb(255, 80, 80))
                        );
                        ui.add_space(10.0);
                        ui.label("Are you sure you want to permanently delete this command?");
                        ui.add_space(15.0);
                        
                        ui.horizontal(|ui| {
                            if ui.button("Cancel").clicked() {
                                self.show_delete_confirmation = false;
                            }
                            ui.add_space(10.0);
                            let delete_btn = egui::Button::new(
                                egui::RichText::new("🗑 Delete Forever")
                                    .color(egui::Color32::WHITE)
                            )
                            .fill(egui::Color32::from_rgb(200, 50, 50));
                            
                            if ui.add(delete_btn).clicked() {
                                if let Some(Selection::Command(cmd_id)) = self.active_selection {
                                    if self.store.is_command_in_workflow(&cmd_id) {
                                        // TODO: Show clearer error?
                                        eprintln!("Cannot delete command as it is part of a workflow");
                                    } else {
                                        self.store.remove_command(&cmd_id);
                                        self.active_selection = None;
                                        self.edited_command = None;
                                    }
                                } else if let Some(Selection::Workflow(wf_id)) = self.active_selection {
                                     self.store.remove_workflow(&wf_id);
                                     self.active_selection = None;
                                     self.edited_workflow = None;
                                }
                                self.show_delete_confirmation = false;
                            }
                        });
                        ui.add_space(5.0);
                    });
                });
        }
        
        // Pending Execution Prompt
        let mut confirmed_pending = false;
        let mut cancelled_pending = false;
        if let Some(pending) = &mut self.pending_execution {
             egui::Window::new("Enter Variables")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                     ui.label("Please provide values for the following variables:");
                     ui.add_space(10.0);
                     
                     egui::Grid::new("prompt_grid").num_columns(2).spacing([10.0, 10.0]).show(ui, |ui| {
                         for var in &mut pending.vars_to_ask {
                             ui.label(&var.key);
                             ui.text_edit_singleline(&mut var.value);
                             ui.end_row();
                         }
                     });
                     
                     ui.add_space(15.0);
                     ui.horizontal(|ui| {
                         if ui.button("Cancel").clicked() {
                             cancelled_pending = true;
                         }
                         if ui.button("Run").clicked() {
                             confirmed_pending = true;
                         }
                     });
                });
        }
        
        if cancelled_pending {
            self.pending_execution = None;
        }
        
        if confirmed_pending {
            if let Some(pending) = self.pending_execution.take() {
                // Merge initial vars with user inputs
                let mut final_vars = pending.initial_vars;
                for v in pending.vars_to_ask {
                    final_vars.insert(v.key, v.value);
                }
                
                if let Some(wf_id) = pending.workflow_id {
                    // Start Workflow
                    if let Some(wf) = self.store.get_workflow(&wf_id) {
                         if let Some(first_cmd_id) = wf.commands.first() {
                             self.active_workflow = Some(ActiveWorkflow {
                                 workflow_id: wf_id,
                                 current_step_index: 0,
                                 current_execution_id: None,
                                 resolved_env: final_vars,
                             });
                             self.perform_execution(*first_cmd_id, None); // Workflow env vars are handled by active_workflow
                         }
                    }
                } else if let Some(cmd_id) = pending.cmd_id {
                    // Start Single Command
                    self.perform_execution(cmd_id, Some(final_vars));
                }
            }
        }

        // Poll for execution updates
        while let Ok((exec_id, update)) = self.execution_rx.try_recv() {
            if let Some(state) = self.executions.iter_mut().find(|e| e.id == exec_id) {
                match update {
                    ExecutionUpdate::Started(_) => {
                        state.is_running = true;
                    }
                    ExecutionUpdate::Stdout(text) => {
                        state.output_buffer.push_str(&text);
                        ctx.request_repaint(); 
                    }
                    ExecutionUpdate::Stderr(text) => {
                        // For now, just append to buffer but maybe wrap in a way we can colorize later?
                        // Or just append [STDERR] prefix?
                        // Let's just append for now, but we really want color.
                        // Since output_buffer is just a string, we can't easily colorize parts of it without parsing.
                        // Let's wrap it in a pseudo-tag for now if we want, or just append.
                        // Actually, let's just push it. The user just wants to SEE it.
                        state.output_buffer.push_str(&text);
                        ctx.request_repaint();
                    }
                    ExecutionUpdate::Exit(code) => {
                        state.is_running = false;
                        state.exit_code = Some(code);
                        state.kill_tx = None; // Clear kill channel
                        
                        state.is_running = false;
                        state.exit_code = Some(code);
                        state.kill_tx = None; // Clear kill channel
                        
                        // Save result
                        let finished_at = chrono::Utc::now();
                        let duration = finished_at.signed_duration_since(state.started_at).num_milliseconds() as u64;
                        
                        let result = switchboard_core::models::ExecutionResult {
                            id: state.id,
                            command_id: state._command_id,
                            host_id: Uuid::nil(), // TODO: Track host ID if needed
                            started_at: state.started_at,
                            finished_at: Some(finished_at),
                            exit_code: Some(code),
                            duration_ms: Some(duration),
                            status: if code == 0 { switchboard_core::models::ExecutionStatus::Completed } else { switchboard_core::models::ExecutionStatus::Failed },
                            log_file: format!("{}.log.gz", state.id),
                        };

                        self.store.add_execution(&result, &state.output_buffer);
                        
                        // Check workflow progress
                        self.check_workflow_progress(exec_id, code);

                        ctx.request_repaint();
                    }
                }
            }
        }

        // Sidebar
        egui::SidePanel::left("sidebar_panel")
            .resizable(true)
            .default_width(self.sidebar_width)
            .show(ctx, |ui| {
                // Top Half: Commands
                ui.vertical(|ui| {
                    // Workflows Section
                     ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Workflows").strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                             if ui.small_button("➕").clicked() {
                                 self.create_new_workflow();
                             }
                        });
                    });
                    let mut workflows = self.store.list_workflows();
                    workflows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                    
                    egui::ScrollArea::vertical()
                    .id_salt("sidebar_workflows_scroll")
                    .max_height(100.0)
                    .show(ui, |ui| {
                        for wf in workflows {
                            ui.horizontal(|ui| {
                                if ui.small_button("▶").clicked() {
                                    self.trigger_workflow_execution(wf.id);
                                }
                                let is_selected = matches!(self.active_selection, Some(Selection::Workflow(id)) if id == wf.id);
                                if ui.selectable_label(is_selected, &wf.name).clicked() {
                                    self.navigate_to(Selection::Workflow(wf.id));
                                    self.edited_workflow = Some(WorkflowEditState::from_workflow(&wf));
                                    self.edited_command = None;
                                }
                            });
                        }
                    });
                    
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Commands").strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                             if ui.small_button("➕").clicked() {
                                 self.create_new_command();
                             }
                        });
                    });
                    // Clone commands to avoid borrow checker issues when calling trigger_command_execution
                    let mut commands = self.store.list_commands();
                    // Sort by creation date (newest first)
                    commands.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                    egui::ScrollArea::vertical()
                        .id_salt("sidebar_commands_scroll")
                        .max_height(ctx.content_rect().height() * 0.5)
                        .show(ui, |ui| {
                            for cmd in commands {
                                ui.horizontal(|ui| {
                                    if ui.small_button("▶").clicked() {
                                        self.trigger_command_execution(cmd.id);
                                    }
                                    
                                    let is_selected = matches!(self.active_selection, Some(Selection::Command(id)) if id == cmd.id);
                                    if ui.selectable_label(is_selected, &cmd.name).clicked() {
                                        self.navigate_to(Selection::Command(cmd.id));
                                        // Initialize edit state
                                        self.edited_command = Some(CommandEditState::from_command(&cmd));
                                    }
                                });
                            }
                        });
                });


            });

        // Right Sidebar: Run History
        egui::SidePanel::right("run_history_panel")
            .resizable(true)
            .default_width(self.sidebar_width)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.heading("Run History");
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("sidebar_executions_scroll")
                        .show(ui, |ui| {
                            let mut execution_to_nav = None;

                            let session_execs: Vec<_> = self.executions.iter()
                                .filter(|e| !e.is_from_history)
                                .collect();
                            let history_execs: Vec<_> = self.executions.iter()
                                .filter(|e| e.is_from_history)
                                .collect();

                            let render_exec = |ui: &mut egui::Ui, exec: &&ExecutionState, nav: &mut Option<Uuid>| {
                                let is_selected = matches!(self.active_selection, Some(Selection::Execution(id)) if id == exec.id);
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    if exec.is_running {
                                        ui.add(egui::Spinner::new().size(12.0));
                                    } else if exec.exit_code == Some(0) {
                                        ui.label("✅");
                                    } else {
                                        ui.label("❌");
                                    }
                                    let time = exec.started_at.with_timezone(&chrono::Local).format("%m/%d %H:%M");
                                    let label = format!("{} ({})", exec.command_name, time);
                                    if ui.selectable_label(is_selected, label).clicked() {
                                        *nav = Some(exec.id);
                                    }
                                });
                            };

                            if !session_execs.is_empty() {
                                ui.label(egui::RichText::new("This Session").small().weak());
                                for exec in session_execs.iter().rev() {
                                    render_exec(ui, exec, &mut execution_to_nav);
                                }
                            }

                            if !history_execs.is_empty() {
                                if !session_execs.is_empty() {
                                    ui.add_space(6.0);
                                }
                                ui.label(egui::RichText::new("History").small().weak());
                                for exec in history_execs.iter().rev() {
                                    render_exec(ui, exec, &mut execution_to_nav);
                                }
                            }

                            if let Some(id) = execution_to_nav {
                                self.navigate_to(Selection::Execution(id));
                            }
                        });
                });
            });

        let mut command_to_run = None;
        let mut workflow_to_run = None;
        let mut jump_to_command = None;
        let mut need_save = false;
        let mut duplicate_cmd = false;

        // Central Panel
        egui::CentralPanel::default().show(ctx, |ui| {
             // Breadcrumb Navigation
             ui.horizontal(|ui| {
                if ui.button("🏠 Home").clicked() {
                    self.save_current_command();
                    self.save_current_workflow();
                    self.navigation_history.clear();
                    self.active_selection = None;
                    self.edited_command = None;
                    self.edited_workflow = None;
                }
                
                // Show last 3 history items
                let history_len = self.navigation_history.len();
                let start_idx = if history_len > 3 { history_len - 3 } else { 0 };
                
                let mut jump_to_history_idx = None;
                
                for (i, selection) in self.navigation_history.iter().enumerate().skip(start_idx) {
                     ui.label(">");
                     let name = match selection {
                        Selection::Command(id) => self.store.get_command(id).map(|c| c.name).unwrap_or_else(|| "Command".into()),
                        Selection::Workflow(id) => self.store.get_workflow(id).map(|w| w.name).unwrap_or_else(|| "Workflow".into()),
                        Selection::Execution(id) => self.executions.iter().find(|e| e.id == *id).map(|e| e.command_name.clone()).unwrap_or_else(|| "Execution".into()),
                     };
                     
                     if ui.button(name).clicked() {
                         jump_to_history_idx = Some(i);
                     }
                }
                
                if let Some(idx) = jump_to_history_idx {
                    // We want to go back TO this item.
                    // This means we pop everything AFTER it, and then pop IT to make it the active selection.
                    // self.navigation_history contains [A, B, C]. We click B (idx 1).
                    // We want history to be [A], and active to be B.
                    // So we need to pop (len - 1 - idx) + 1 times?
                    // No.
                    // If we have [A, B, C] and active is D.
                    // Click B.
                    // 1. Pop D (current active).
                    // 2. Pop C.
                    // 3. Pop B -> becomes active.
                    
                    let pop_count = self.navigation_history.len() - idx;
                    for _ in 0..pop_count {
                        self.navigate_back();
                    }
                }

                if let Some(selection) = self.active_selection {
                    ui.label(">");
                    match selection {
                        Selection::Command(id) => {
                             let name = self.store.get_command(&id).map(|c| c.name).unwrap_or_else(|| "Unknown Command".into());
                             ui.label(egui::RichText::new(name).strong());
                        }
                        Selection::Workflow(id) => {
                             let name = self.store.get_workflow(&id).map(|w| w.name).unwrap_or_else(|| "Unknown Workflow".into());
                             ui.label(egui::RichText::new(name).strong());
                        }
                         Selection::Execution(id) => {
                            let name = self.executions.iter().find(|e| e.id == id).map(|e| e.command_name.clone()).unwrap_or_else(|| "Execution".into());
                            ui.label(format!("Run: {}", name));
                        }
                    }
                }
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.navigation_history.is_empty() {
                         if ui.button("⬅ Back").clicked() {
                             self.navigate_back();
                         }
                    }
                });
            });
            ui.separator();

            match self.active_selection {
                Some(Selection::Workflow(_wf_id)) => {
                    if let Some(edit_state) = &mut self.edited_workflow {
                         ui.horizontal(|ui| {
                             ui.heading("Edit Workflow");
                             if ui.button("▶ Run Workflow").clicked() {
                                 if let Some(Selection::Workflow(id)) = self.active_selection {
                                     workflow_to_run = Some(id);
                                 }
                             }
                             if ui.button("🗑 Delete").clicked() {
                                 self.show_delete_confirmation = true;
                             }
                         });
                         ui.separator();
                         
                         ui.label("Name:");
                         if ui.text_edit_singleline(&mut edit_state.name).changed() {
                             need_save = true;
                         }
                         
                         ui.label("Description:");
                         if ui.text_edit_singleline(&mut edit_state.description).changed() {
                             need_save = true;
                         }
                         ui.separator();
                         
                         ui.collapsing("Environment Configuration (Overrides)", |ui| {
                            let mut remove_idx = None;
                            for (i, var) in edit_state.env_vars.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    if ui.text_edit_singleline(&mut var.key).on_hover_text("Key").changed() { need_save = true; }
                                    ui.label("=");
                                    if ui.text_edit_singleline(&mut var.value).on_hover_text("Value").changed() { need_save = true; }
                                    if ui.checkbox(&mut var.ask_user, "Ask").on_hover_text("Ask user at runtime").changed() { need_save = true; }
                                    if ui.button("❌").clicked() { remove_idx = Some(i); }
                                });
                            }
                            if let Some(i) = remove_idx {
                                edit_state.env_vars.remove(i);
                                need_save = true;
                            }
                            if ui.button("➕ Add Override").clicked() {
                                edit_state.env_vars.push(switchboard_core::models::EnvVar {
                                    key: "".to_string(),
                                    value: "".to_string(),
                                    ask_user: false,
                                });
                                need_save = true;
                            }
                         });
                         ui.separator();

                         ui.heading("Workflow Steps");
                         
                         // List current commands
                         let all_commands = self.store.list_commands();
                         
                         let mut to_remove_idx = None;
                         
                         for (idx, cmd_id) in edit_state.commands.iter().enumerate() {
                             if let Some(cmd) = all_commands.iter().find(|c| c.id == *cmd_id) {
                                 ui.horizontal(|ui| {
                                     if ui.small_button(format!("{}", cmd.name)).on_hover_text("Jump to Command").clicked() {
                                         jump_to_command = Some(*cmd_id);
                                     }
                                     if ui.small_button("❌").clicked() {
                                         to_remove_idx = Some(idx);
                                     }
                                 });
                             }
                         }
                         
                         if let Some(idx) = to_remove_idx {
                             edit_state.commands.remove(idx);
                             need_save = true;
                         }
                         
                         egui::ComboBox::from_id_salt("add_command_combo")
                             .selected_text("Add command...")
                             .show_ui(ui, |ui| {
                                 for cmd in all_commands {
                                     if ui.selectable_label(false, &cmd.name).clicked() {
                                         edit_state.commands.push(cmd.id);
                                         need_save = true;
                                     }
                                 }
                             });
                    }
                },
                Some(Selection::Command(_cmd_id)) => {

                    // COMMAND EDITOR VIEW
                    if let Some(edit_state) = &mut self.edited_command {
                        ui.horizontal(|ui| {
                            ui.heading("Edit Command");
                        });
                        
                        // Action menu bar
                        ui.horizontal(|ui| {
                            ui.spacing_mut().button_padding = egui::vec2(8.0, 4.0);
                            
                            if ui.button("▶ Run").clicked() {
                                if let Some(Selection::Command(id)) = self.active_selection {
                                    command_to_run = Some(id);
                                }
                            }
                            
                            if ui.button("📋 Duplicate").clicked() {
                                duplicate_cmd = true;
                            }
                            
                            if ui.button("🗑 Delete").clicked() {
                                self.show_delete_confirmation = true;
                            }
                        });
                        ui.separator();

                        egui::ScrollArea::vertical()
                            .id_salt("editor_scroll")
                            .show(ui, |ui| {
                                egui::Grid::new("metadata_grid").num_columns(2).spacing([10.0, 10.0]).show(ui, |ui| {
                                    ui.label("Name:");
                                    if ui.text_edit_singleline(&mut edit_state.name).changed() {
                                        need_save = true;
                                    }
                                    ui.end_row();

                                    ui.label("Description:");
                                    if ui.text_edit_singleline(&mut edit_state.description).changed() {
                                        need_save = true;
                                    }
                                    ui.end_row();

                                    ui.label("Execute:");
                                    ui.horizontal(|ui| {
                                        if ui.checkbox(&mut edit_state.is_local, "Run Locally").changed() {
                                            need_save = true;
                                        }
                                        if ui.checkbox(&mut edit_state.background, "Run in background (nohup)").changed() {
                                            need_save = true;
                                        }
                                    });
                                    ui.end_row();

                                    if !edit_state.is_local {
                                        ui.label("User:");
                                        if ui.text_edit_singleline(&mut edit_state.user).changed() {
                                            need_save = true;
                                        }
                                        ui.end_row();
    
                                        ui.label("Host:");
                                        if ui.text_edit_singleline(&mut edit_state.host).changed() {
                                            need_save = true;
                                        }
                                        ui.end_row();
                                    }
                                    
                                    ui.label("Working Dir:");
                                    if ui.text_edit_singleline(&mut edit_state.working_directory).changed() {
                                        need_save = true;
                                    }
                                    ui.end_row();
                                });
                                
                                ui.separator();
                                ui.collapsing("Environment Variables", |ui| {
                                    let mut remove_idx = None;
                                    for (i, var) in edit_state.env_vars.iter_mut().enumerate() {
                                        ui.horizontal(|ui| {
                                            if ui.text_edit_singleline(&mut var.key).on_hover_text("Key").changed() { need_save = true; }
                                            ui.label("=");
                                            if ui.text_edit_singleline(&mut var.value).on_hover_text("Value").changed() { need_save = true; }
                                            if ui.checkbox(&mut var.ask_user, "Ask").on_hover_text("Ask user at runtime").changed() { need_save = true; }
                                            if ui.button("❌").clicked() { remove_idx = Some(i); }
                                        });
                                    }
                                    if let Some(i) = remove_idx {
                                        edit_state.env_vars.remove(i);
                                        need_save = true;
                                    }
                                    if ui.button("➕ Add Variable").clicked() {
                                        edit_state.env_vars.push(switchboard_core::models::EnvVar {
                                            key: "".to_string(),
                                            value: "".to_string(),
                                            ask_user: false,
                                        });
                                        need_save = true;
                                    }
                                });
                                
                                ui.separator();
                                ui.label("Script (Bash):");
                                
                                let available_height = ui.available_height();
                                if ui.add_sized(
                                    [ui.available_width(), available_height - 30.0],
                                    egui::TextEdit::multiline(&mut edit_state.script)
                                        .code_editor()
                                        .lock_focus(false),
                                ).changed() {
                                    need_save = true;
                                }
                            });
                        
                    } else {
                        ui.label("Command not found (deleted?)");
                    }
                },
                Some(Selection::Execution(exec_id)) => {
                    // Load logs if needed
                    if let Some(state) = self.executions.iter_mut().find(|e| e.id == exec_id) {
                        if !state.output_loaded && !state.is_running {
                             if let Some(logs) = self.store.get_execution_log(&exec_id) {
                                 state.output_buffer = logs;
                                 state.output_loaded = true;
                             }
                        }
                    }

                    // EXECUTION OUTPUT VIEW
                    if let Some(state) = self.executions.iter().find(|e| e.id == exec_id) {
                         ui.horizontal(|ui| {
                            ui.heading(format!("Run: {}", state.command_name));
                            ui.add_space(10.0);

                            if ui.small_button("📋 Copy ID").on_hover_text(exec_id.to_string()).clicked() {
                                ui.output_mut(|o| o.commands.push(egui::OutputCommand::CopyText(exec_id.to_string())));
                            }
                            ui.add_space(6.0);

                            if state.is_running {
                                ui.spinner();
                                ui.label("Running");
                                
                                // Kill button
                                if ui.button("⏹ Kill").clicked() {
                                    if let Some(kill_tx) = &state.kill_tx {
                                        let _ = kill_tx.send(());
                                    }
                                }
                            } else if let Some(code) = state.exit_code {
                                if code == 0 {
                                    ui.label(egui::RichText::new("✅ Success").color(egui::Color32::from_rgb(100, 200, 100)));
                                    
                                    if state.is_local {
                                        if ui.button("📂 Open Directory").clicked() {
                                            let dir = state.working_directory.clone().unwrap_or_else(|| ".".to_string());
                                            let _ = std::process::Command::new("open")
                                                .arg(dir)
                                                .spawn();
                                        }
                                    }
                                } else {
                                    ui.label(egui::RichText::new(format!("❌ Exit Code: {}", code)).color(egui::Color32::from_rgb(255, 100, 100)));
                                }
                            }
                        });
                        ui.separator();
                        
                        egui::Frame::new()
                            .fill(egui::Color32::BLACK)
                            .inner_margin(8.0)
                            .corner_radius(4.0)
                            .show(ui, |ui| {
                                egui::ScrollArea::vertical()
                                    .id_salt("execution_log_scroll")
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.set_min_height(ui.available_height());
                                        
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&state.output_buffer)
                                                    .monospace()
                                                    .size(11.0)
                                                    .color(egui::Color32::WHITE)
                                            )
                                            .wrap()
                                        );
                                    });
                            });
                    } else {
                        ui.label("Execution not found");
                    }
                },
                None => {
                    ui.centered_and_justified(|ui| {
                        ui.label("Select a command to edit, or a run to view output.");
                    });
                }
            }
        });

        if need_save {
            self.save_current_command();
            self.save_current_workflow();
        }

        if duplicate_cmd {
            if let Some(Selection::Command(cmd_id)) = self.active_selection {
                if let Some(cmd) = self.store.get_command(&cmd_id) {
                    let new_id = Uuid::new_v4();
                    let mut new_cmd = cmd.clone();
                    new_cmd.id = new_id;
                    new_cmd.name = format!("{} (Copy)", cmd.name);
                    new_cmd.created_at = chrono::Utc::now();
                    save_command(&self.store, &new_cmd);
                    self.active_selection = Some(Selection::Command(new_id));
                    self.edited_command = Some(CommandEditState::from_command(&new_cmd));
                }
            }
        }

        if let Some(id) = command_to_run {
            self.trigger_command_execution(id);
        }
        
        if let Some(id) = workflow_to_run {
            self.trigger_workflow_execution(id);
        }
        
        if let Some(cmd_id) = jump_to_command {
            if let Some(cmd) = self.store.get_command(&cmd_id) {
                 self.active_selection = Some(Selection::Command(cmd_id));
                 self.edited_command = Some(CommandEditState::from_command(&cmd));
            }
        }
    }
}
