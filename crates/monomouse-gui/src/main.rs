use anyhow::Result;
use eframe::egui;
use monomouse_core::{Config, Grid, Monitor};
use uuid::Uuid;

fn main() -> Result<()> {
    let config = Config::load().unwrap_or_else(|_| Config {
        machine: monomouse_core::Machine::new("local".to_string(), true),
        grid: Grid::new(),
        network: Default::default(),
        security: Default::default(),
    });

    // Detect local monitors
    let local_monitors = monomouse_input::detect_monitors().unwrap_or_default();

    let app = GridBuilderApp::new(config, local_monitors);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_title("MonoMouse Grid Builder"),
        ..Default::default()
    };

    eframe::run_native(
        "MonoMouse Grid Builder",
        options,
        Box::new(|_cc| Ok(Box::new(app))),
    )
    .map_err(|e| anyhow::anyhow!("GUI error: {e}"))
}

struct GridBuilderApp {
    config: Config,
    grid: Grid,
    unplaced_monitors: Vec<Monitor>,
    grid_cols: u32,
    grid_rows: u32,
    selected_cell: Option<(u32, u32)>,
    status_message: String,
}

impl GridBuilderApp {
    fn new(config: Config, local_monitors: Vec<Monitor>) -> Self {
        let grid = config.grid.clone();

        let placed_ids: Vec<Uuid> = grid
            .monitors
            .iter()
            .filter(|m| m.grid_col.is_some() && m.grid_row.is_some())
            .map(|m| m.id)
            .collect();

        let mut unplaced = Vec::new();
        for mon in &local_monitors {
            if !placed_ids.contains(&mon.id)
                && !grid.monitors.iter().any(|m| m.name == mon.name)
            {
                unplaced.push(mon.clone());
            }
        }

        let max_col = grid
            .monitors
            .iter()
            .filter_map(|m| m.grid_col)
            .max()
            .unwrap_or(0);
        let max_row = grid
            .monitors
            .iter()
            .filter_map(|m| m.grid_row)
            .max()
            .unwrap_or(0);

        Self {
            config,
            grid,
            unplaced_monitors: unplaced,
            grid_cols: (max_col + 2).max(4),
            grid_rows: (max_row + 2).max(2),
            selected_cell: None,
            status_message: String::new(),
        }
    }

    /// Get the target cell for placing a monitor:
    /// use the selected cell if it's empty, otherwise find the first empty cell.
    fn target_cell(&self) -> Option<(u32, u32)> {
        if let Some((col, row)) = self.selected_cell {
            if self.grid.monitor_at(col, row).is_none() {
                return Some((col, row));
            }
        }
        // Fallback to first empty cell
        for row in 0..self.grid_rows {
            for col in 0..self.grid_cols {
                if self.grid.monitor_at(col, row).is_none() {
                    return Some((col, row));
                }
            }
        }
        None
    }

    fn place_monitor(&mut self, mon: &Monitor) {
        if let Some((col, row)) = self.target_cell() {
            let mut m = mon.clone();
            m.grid_col = Some(col);
            m.grid_row = Some(row);

            self.unplaced_monitors.retain(|u| u.id != mon.id);
            self.grid.monitors.retain(|u| u.id != mon.id);
            self.grid.monitors.push(m);
            self.grid.rebuild_transitions();
            self.status_message = format!("Placed {} at ({}, {})", mon.name, col, row);
        }
    }
}

impl eframe::App for GridBuilderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Top panel with toolbar
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("MonoMouse Grid Builder");
                ui.separator();

                if ui.button("Save Config").clicked() {
                    self.config.grid = self.grid.clone();
                    match self.config.save() {
                        Ok(()) => self.status_message = "Config saved!".to_string(),
                        Err(e) => self.status_message = format!("Save failed: {e}"),
                    }
                }

                if ui.button("Rebuild Transitions").clicked() {
                    self.grid.rebuild_transitions();
                    self.status_message = format!(
                        "Rebuilt {} transitions",
                        self.grid.transitions.len()
                    );
                }

                ui.separator();
                ui.label("Grid size:");
                ui.add(egui::DragValue::new(&mut self.grid_cols).range(1..=10).prefix("cols: "));
                ui.add(egui::DragValue::new(&mut self.grid_rows).range(1..=10).prefix("rows: "));

                if !self.status_message.is_empty() {
                    ui.separator();
                    ui.label(&self.status_message);
                }
            });
        });

        // Left panel: unplaced monitors
        egui::SidePanel::left("unplaced")
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.heading("Unplaced Monitors");
                ui.separator();

                // Hint about selected cell
                if let Some((col, row)) = self.selected_cell {
                    if self.grid.monitor_at(col, row).is_none() {
                        ui.label(format!("Target: cell ({}, {})", col, row));
                        ui.separator();
                    }
                }

                let unplaced_in_grid: Vec<Monitor> = self
                    .grid
                    .monitors
                    .iter()
                    .filter(|m| m.grid_col.is_none() || m.grid_row.is_none())
                    .cloned()
                    .collect();

                let all_unplaced: Vec<Monitor> = self
                    .unplaced_monitors
                    .iter()
                    .chain(unplaced_in_grid.iter())
                    .cloned()
                    .collect();

                if all_unplaced.is_empty() {
                    ui.label("All monitors placed!");
                } else {
                    // Collect which monitor to place (can't mutate self inside the loop)
                    let mut to_place: Option<Monitor> = None;

                    for mon in &all_unplaced {
                        let text = format!(
                            "{}\n{}x{} (machine: {})",
                            mon.name,
                            mon.width,
                            mon.height,
                            &mon.machine_id.to_string()[..8]
                        );

                        let response = ui.add(
                            egui::Button::new(&text)
                                .min_size(egui::vec2(180.0, 50.0)),
                        );

                        if response.clicked() {
                            to_place = Some(mon.clone());
                        }
                    }

                    if let Some(mon) = to_place {
                        self.place_monitor(&mon);
                    }
                }
            });

        // Right panel: info about selected cell
        egui::SidePanel::right("info")
            .default_width(250.0)
            .show(ctx, |ui| {
                ui.heading("Monitor Info");
                ui.separator();

                if let Some((col, row)) = self.selected_cell {
                    if let Some(mon) = self.grid.monitor_at(col, row).cloned() {
                        ui.label(format!("Name: {}", mon.name));
                        ui.label(format!("Resolution: {}x{}", mon.width, mon.height));
                        ui.label(format!("Position: +{}+{}", mon.x, mon.y));
                        ui.label(format!("Scale: {:.1}x", mon.scale));
                        ui.label(format!("Grid: ({}, {})", col, row));
                        ui.label(format!("Machine: {}", &mon.machine_id.to_string()[..8]));

                        ui.separator();
                        ui.label("Transitions:");
                        for t in &self.grid.transitions {
                            if t.from_monitor == mon.id {
                                let to_name = self
                                    .grid
                                    .monitors
                                    .iter()
                                    .find(|m| m.id == t.to_monitor)
                                    .map(|m| m.name.as_str())
                                    .unwrap_or("?");
                                ui.label(format!("  {:?} -> {}", t.from_edge, to_name));
                            }
                        }

                        ui.separator();
                        if ui.button("Remove from grid").clicked() {
                            let removed = self
                                .grid
                                .monitors
                                .iter()
                                .find(|m| m.id == mon.id)
                                .cloned();
                            self.grid.monitors.retain(|m| m.id != mon.id);
                            if let Some(mut m) = removed {
                                m.grid_col = None;
                                m.grid_row = None;
                                self.unplaced_monitors.push(m);
                            }
                            self.grid.rebuild_transitions();
                            self.selected_cell = None;
                        }
                    } else {
                        ui.label(format!("Empty cell ({}, {})", col, row));
                        ui.label("Click a monitor in the left panel to place it here.");
                    }
                } else {
                    ui.label("Select a grid cell, then click a monitor to place it there.");
                }
            });

        // Central panel: the grid
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Monitor Grid");
            ui.label("1. Click an empty cell to select it  2. Click a monitor on the left to place it there");
            ui.separator();

            let available = ui.available_size();
            let cell_width = (available.x / self.grid_cols as f32).min(200.0);
            let cell_height = (available.y / self.grid_rows as f32).min(150.0);

            for row in 0..self.grid_rows {
                ui.horizontal(|ui| {
                    for col in 0..self.grid_cols {
                        let is_selected = self.selected_cell == Some((col, row));
                        let mon = self.grid.monitor_at(col, row);

                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(cell_width - 4.0, cell_height - 4.0),
                            egui::Sense::click(),
                        );

                        let painter = ui.painter();

                        let bg_color = if is_selected && mon.is_none() {
                            egui::Color32::from_rgb(50, 150, 50) // Green for selected empty cell
                        } else if is_selected {
                            egui::Color32::from_rgb(70, 130, 180) // Blue for selected occupied cell
                        } else if mon.is_some() {
                            egui::Color32::from_rgb(60, 60, 80)
                        } else {
                            egui::Color32::from_rgb(30, 30, 40)
                        };

                        painter.rect_filled(rect, 4.0, bg_color);
                        painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::GRAY), egui::StrokeKind::Outside);

                        if let Some(mon) = mon {
                            let text = format!(
                                "{}\n{}x{}",
                                mon.name, mon.width, mon.height
                            );
                            painter.text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                text,
                                egui::FontId::proportional(12.0),
                                egui::Color32::WHITE,
                            );
                        } else {
                            let label = if is_selected {
                                "[ TARGET ]".to_string()
                            } else {
                                format!("({}, {})", col, row)
                            };
                            painter.text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                label,
                                egui::FontId::proportional(10.0),
                                if is_selected {
                                    egui::Color32::WHITE
                                } else {
                                    egui::Color32::from_rgb(80, 80, 80)
                                },
                            );
                        }

                        if response.clicked() {
                            self.selected_cell = Some((col, row));
                        }
                    }
                });
            }

            ui.separator();
            ui.label(format!(
                "Total monitors: {} | Transitions: {} | Grid: {}x{}",
                self.grid.monitors.iter().filter(|m| m.grid_col.is_some()).count(),
                self.grid.transitions.len(),
                self.grid_cols,
                self.grid_rows,
            ));
        });
    }
}
