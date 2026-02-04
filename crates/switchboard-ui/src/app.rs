use eframe::{egui, App, Frame};
use std::sync::mpsc::{Receiver, channel, Sender};
use switchboard_core::{
    CommandExecutor, CommandStore, ExecutionUpdate,
    save_command,
};
use uuid::Uuid;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Selection {
    Command(Uuid),
    Execution(Uuid),
}

pub struct ExecutionState {
    pub id: Uuid,
    pub _command_id: Uuid,
    pub command_name: String,
    pub output_buffer: String,
    pub is_running: bool,
    pub exit_code: Option<i32>,
    pub kill_tx: Option<Sender<()>>,
}

#[derive(Clone, Default)]
struct CommandEditState {
    name: String,
    description: String,
    host: String,
    user: String,
    working_directory: String,
    script: String,
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
        }
    }
    
    fn apply_to_command(&self, cmd: &mut switchboard_core::models::Command) {
        cmd.name = self.name.clone();
        cmd.description = if self.description.is_empty() { None } else { Some(self.description.clone()) };
        cmd.host = if self.host.is_empty() { None } else { Some(self.host.clone()) };
        cmd.user = if self.user.is_empty() { None } else { Some(self.user.clone()) };
        cmd.working_directory = if self.working_directory.is_empty() { None } else { Some(self.working_directory.clone()) };
        cmd.script = self.script.clone();
    }
}

pub struct SwitchboardApp {
    store: CommandStore,
    executor: Box<dyn CommandExecutor>,
    
    // Selection State
    active_selection: Option<Selection>,
    
    // UI State
    sidebar_width: f32,
    show_delete_confirmation: bool,

    // Editing State
    edited_command: Option<CommandEditState>,
    
    // Execution State
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

        // Execution channel
        let (exec_tx, exec_rx) = channel();

        // SSH Executor
        use switchboard_core::SshExecutor;

        Self {
            store,
            executor: Box::new(SshExecutor),
            active_selection: None,
            sidebar_width: 250.0,
            show_delete_confirmation: false,
            edited_command: None,
            executions: Vec::new(),
            execution_tx: exec_tx,
            execution_rx: exec_rx,
        }
    }

    fn trigger_command_execution(&mut self, cmd_id: Uuid) {
        // Save first (only if we are currently editing THIS command)
        if let Some(Selection::Command(active_id)) = self.active_selection {
            if active_id == cmd_id {
                self.save_current_command();
            }
        }

        // Fetch command to run
        if let Some(cmd) = self.store.get_command(&cmd_id) {
             use switchboard_core::{Host, AuthMethod};
             
             // Detect current user for "localhost" test
             let default_user = std::env::var("USER").unwrap_or_else(|_| "root".into());
             
             let hostname = cmd.host.clone().unwrap_or_else(|| "localhost".to_string());
             let username = cmd.user.clone().unwrap_or(default_user);
             let name = if cmd.host.is_some() { "Remote".into() } else { "Localhost".into() };

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
            };
            self.executions.push(state);
            
            // Switch view
            self.active_selection = Some(Selection::Execution(exec_id));
            
            // Run
            if let Err(e) = self.executor.execute(&cmd, &dummy_host, cb, kill_rx) {
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

    fn create_new_command(&mut self) {
        let id = Uuid::new_v4();
        let cmd = switchboard_core::models::Command {
            id,
            name: "New Command".to_string(),
            description: None,
            script: "".to_string(),
            working_directory: None,
            environment: std::collections::HashMap::new(),
            host: None,
            user: None,
            target_hosts: Vec::new(),
            created_at: chrono::Utc::now(),
            source_path: None,
        };

        save_command(&self.store, &cmd);
        self.active_selection = Some(Selection::Command(id));
        self.edited_command = Some(CommandEditState::from_command(&cmd));
    }
}

impl App for SwitchboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
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
                                    self.store.remove_command(&cmd_id);
                                    self.active_selection = None;
                                    self.edited_command = None;
                                }
                                self.show_delete_confirmation = false;
                            }
                        });
                        ui.add_space(5.0);
                    });
                });
        }

        // Poll for execution updates
        while let Ok((exec_id, update)) = self.execution_rx.try_recv() {
            if let Some(state) = self.executions.iter_mut().find(|e| e.id == exec_id) {
                match update {
                    ExecutionUpdate::Started(_) => {
                        state.is_running = true;
                    }
                    ExecutionUpdate::Output(text) => {
                        state.output_buffer.push_str(&text);
                        ctx.request_repaint(); 
                    }
                    ExecutionUpdate::Exit(code) => {
                        state.is_running = false;
                        state.exit_code = Some(code);
                        state.kill_tx = None; // Clear kill channel
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
                    ui.horizontal(|ui| {
                        ui.heading("Commands");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                             if ui.button("➕").clicked() {
                                 self.create_new_command();
                             }
                        });
                    });
                    ui.separator();
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
                                        self.active_selection = Some(Selection::Command(cmd.id));
                                        // Initialize edit state
                                        self.edited_command = Some(CommandEditState::from_command(&cmd));
                                    }
                                });
                            }
                        });
                });

                ui.separator();

                // Bottom Half: Executions
                ui.vertical(|ui| {
                    ui.heading("Run History");
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("sidebar_executions_scroll")
                        .show(ui, |ui| {
                             // Show in reverse order (newest first)
                             for exec in self.executions.iter().rev() {
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
                                     
                                     let label = format!("{} ({})", exec.command_name, exec.id.to_string().chars().take(4).collect::<String>());
                                     if ui.selectable_label(is_selected, label).clicked() {
                                         self.active_selection = Some(Selection::Execution(exec.id));
                                     }
                                 });
                             }
                        });
                });
            });

        let mut command_to_run = None;
        let mut need_save = false;
        let mut duplicate_cmd = false;

        // Central Panel
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_selection {
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

                                    ui.label("Host:");
                                    if ui.text_edit_singleline(&mut edit_state.host).changed() {
                                        need_save = true;
                                    }
                                    ui.end_row();

                                    ui.label("User:");
                                    if ui.text_edit_singleline(&mut edit_state.user).changed() {
                                        need_save = true;
                                    }
                                    ui.end_row();
                                    
                                    ui.label("Working Dir:");
                                    if ui.text_edit_singleline(&mut edit_state.working_directory).changed() {
                                        need_save = true;
                                    }
                                    ui.end_row();
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
                    // EXECUTION OUTPUT VIEW
                    if let Some(state) = self.executions.iter().find(|e| e.id == exec_id) {
                         ui.horizontal(|ui| {
                            ui.heading(format!("Run: {}", state.command_name));
                            ui.add_space(10.0);
                            
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
    }
}
