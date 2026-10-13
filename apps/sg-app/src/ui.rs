//! The egui panels: controls, formation timeline and data card.

use crate::cards::{self, Card};
use crate::state::{run_simulation, AppState, SimulationTargets, DEFAULT_PLAYBACK_SECS};
use bevy::prelude::*;
use bevy_egui::egui::{self, Align2, Color32, RichText};
use bevy_egui::EguiContexts;
use sg_render::scene::system_extent;
use sg_render::{PointerCaptured, ScaleMode, Target};
use sg_schema::{Fidelity, PresetId};
use sg_sim::{preset_availability, PresetAvailability, ALL_PRESETS, MODEL_VERSION};

fn preset_name(preset: PresetId) -> &'static str {
    match preset {
        PresetId::SunLike => "Sun-like",
        PresetId::RedDwarf => "Red dwarf",
        PresetId::HotJupiter => "Hot Jupiter",
        PresetId::MassiveAStar => "Massive A star",
        PresetId::MetalPoor => "Metal-poor",
        PresetId::Circumbinary => "Circumbinary (Kepler-16-like)",
        PresetId::STypeBinary => "S-type binary (α Cen-like)",
        PresetId::HierarchicalTriple => "Hierarchical triple",
        PresetId::Random => "Random",
    }
}

const MUTED: Color32 = Color32::from_rgb(150, 156, 170);

fn controls(ui: &mut egui::Ui, state: &mut AppState, targets: &mut SimulationTargets) {
    ui.label(RichText::new(format!("Classic tier · model {MODEL_VERSION}")).color(MUTED));
    ui.separator();

    egui::ComboBox::from_label("Preset")
        .selected_text(preset_name(state.preset))
        .show_ui(ui, |ui| {
            for preset in ALL_PRESETS {
                match preset_availability(preset, Fidelity::Classic) {
                    PresetAvailability::Available => {
                        if ui
                            .selectable_label(state.preset == preset, preset_name(preset))
                            .clicked()
                        {
                            state.apply_preset(preset);
                        }
                    }
                    PresetAvailability::Unavailable(reason) => {
                        ui.add_enabled(false, egui::Button::new(preset_name(preset)))
                            .on_disabled_hover_text(reason);
                    }
                }
            }
        });

    ui.horizontal(|ui| {
        ui.label("Seed");
        ui.add(egui::DragValue::new(&mut state.params.seed).range(1..=u64::from(u32::MAX)));
        if ui.button("New seed").clicked() {
            state.next_seed();
        }
    });

    match state.mass_mut() {
        Some(mass) => {
            ui.add(
                egui::Slider::new(mass, 0.2..=1.5)
                    .text("Star mass (M☉)")
                    .fixed_decimals(2),
            );
        }
        None => {
            ui.label(RichText::new("Star mass: drawn at random (0.7–1.4 M☉)").color(MUTED));
        }
    }

    let mut fixed_age = state.params.age_gyr.is_some();
    ui.horizontal(|ui| {
        if ui.checkbox(&mut fixed_age, "Fix age").changed() {
            state.params.age_gyr = fixed_age.then_some(4.6);
        }
        if let Some(age) = state.params.age_gyr.as_mut() {
            ui.add(
                egui::Slider::new(age, 0.5..=10.0)
                    .suffix(" Gyr")
                    .fixed_decimals(1),
            );
        } else {
            ui.label(RichText::new("sampled, 1–6 Gyr").color(MUTED));
        }
    });

    ui.collapsing("Accretion model", |ui| {
        let c = &mut state.params.formation.classic;
        ui.add(
            egui::Slider::new(&mut c.dust_density_coeff, 5.0e-4..=6.0e-3)
                .logarithmic(true)
                .text("Dust density A"),
        )
        .on_hover_text("More dust: more and heavier planets. StarGen: 0.002.");
        ui.add(egui::Slider::new(&mut c.gas_dust_ratio, 10.0..=150.0).text("Gas/dust ratio K"))
            .on_hover_text("How much gas a giant can capture per unit of dust. StarGen: 50.");
        ui.add(
            egui::Slider::new(&mut c.crit_mass_coeff_msun, 2.0e-6..=6.0e-5)
                .logarithmic(true)
                .text("Critical mass B (M☉)"),
        )
        .on_hover_text("Mass above which a planet captures gas, at 1 AU. StarGen: 1.2e-5.");
        ui.add(egui::Slider::new(&mut c.cloud_eccentricity, 0.0..=0.5).text("Cloud eccentricity"))
            .on_hover_text("Widens each nucleus's reach. StarGen: 0.2.");
        ui.add(
            egui::Slider::new(&mut c.eccentricity_coeff, 0.02..=0.3).text("Eccentricity exponent"),
        )
        .on_hover_text("Larger: more eccentric nuclei. StarGen: 0.077.");
        ui.checkbox(&mut state.params.toggles.atmospheres, "Compute atmospheres");
        if ui.button("Reset to StarGen's values").clicked() {
            *c = Default::default();
        }
    });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let simulate =
            egui::Button::new(RichText::new("Simulate").strong()).min_size(egui::vec2(120.0, 28.0));
        if ui.add(simulate).clicked() {
            run_simulation(state, targets);
        }
        ui.checkbox(&mut state.play_on_simulate, "Play formation");
    });
    if let Some(error) = &state.error {
        ui.colored_label(Color32::from_rgb(240, 110, 100), error);
    }

    ui.separator();
    ui.label(RichText::new("View").strong());
    let mut mode = targets.settings.scale.to;
    ui.horizontal(|ui| {
        for candidate in ScaleMode::ALL {
            ui.radio_value(&mut mode, candidate, candidate.label());
        }
    });
    if mode != targets.settings.scale.to {
        targets.settings.scale.go_to(mode);
        // Keep the system framed in the new scale unless following a body.
        if targets.selection.0.is_none() {
            if let Some(result) = targets.view.result() {
                let settled = sg_render::ScaleBlend::settled(mode);
                targets.camera.frame(system_extent(result, &settled));
            }
        }
    }
    ui.horizontal(|ui| {
        ui.checkbox(&mut targets.settings.show_orbits, "Orbits");
        ui.checkbox(&mut targets.settings.show_habitable_zone, "Habitable zone");
        ui.checkbox(&mut targets.playback.orbits_running, "Orbital motion");
    });
    ui.add(
        egui::Slider::new(&mut targets.playback.days_per_second, 1.0..=3000.0)
            .logarithmic(true)
            .text("days / second"),
    );
    if ui.button("Frame system").clicked() {
        targets.selection.0 = None;
        if let Some(result) = targets.view.result() {
            targets
                .camera
                .frame(system_extent(result, &targets.settings.scale));
        }
    }

    if let Some(result) = targets.view.result() {
        for warning in &result.diagnostics.warnings {
            ui.colored_label(Color32::from_rgb(235, 190, 90), warning);
        }
    }
    ui.add_space(2.0);
    ui.label(
        RichText::new("Drag to rotate · scroll to zoom · click a body for its card")
            .small()
            .color(MUTED),
    );
}

fn timeline(ui: &mut egui::Ui, targets: &mut SimulationTargets) {
    let end = targets.view.timeline_end_myr();
    if end <= 0.0 {
        ui.label("No formation timeline.");
        return;
    }
    let (events, bodies_now) = match targets.view.result() {
        Some(result) => (
            result.timeline.events.len(),
            sg_render::display::bodies_at(result, targets.playback.t_myr).len(),
        ),
        None => (0, 0),
    };

    ui.horizontal(|ui| {
        let at_end = targets.playback.t_myr >= end;
        let label = if targets.playback.playing {
            "Pause"
        } else {
            "Play"
        };
        if ui
            .add_sized([56.0, 22.0], egui::Button::new(label))
            .clicked()
        {
            if at_end {
                targets.playback.t_myr = 0.0;
            }
            targets.playback.playing = !targets.playback.playing;
        }
        if ui.button("Restart").clicked() {
            targets.playback.t_myr = 0.0;
            targets.playback.playing = true;
        }
        if ui.button("End").clicked() {
            targets.playback.t_myr = end;
            targets.playback.playing = false;
        }
        ui.spacing_mut().slider_width = 420.0;
        let slider = egui::Slider::new(&mut targets.playback.t_myr, 0.0..=end)
            .suffix(" Myr")
            .fixed_decimals(2);
        if ui.add(slider).dragged() {
            targets.playback.playing = false;
        }
        // Speed as a multiple of the default playback duration.
        let base = end / DEFAULT_PLAYBACK_SECS;
        let mut speed = targets.playback.myr_per_second / base;
        ui.spacing_mut().slider_width = 90.0;
        if ui
            .add(
                egui::Slider::new(&mut speed, 0.1..=8.0)
                    .logarithmic(true)
                    .suffix("×")
                    .fixed_decimals(1),
            )
            .changed()
        {
            targets.playback.myr_per_second = speed * base;
        }
    });
    let phase = if targets.playback.t_myr >= end {
        "finished system".to_string()
    } else {
        format!("{bodies_now} embryos")
    };
    ui.label(
        RichText::new(format!(
            "{phase} · {events} formation events · Classic times are nominal, not physical"
        ))
        .small()
        .color(MUTED),
    );
}

fn card(ui: &mut egui::Ui, card: &Card) {
    ui.label(RichText::new(&card.subtitle).color(MUTED));
    for section in &card.sections {
        ui.add_space(6.0);
        ui.label(RichText::new(section.heading).strong());
        egui::Grid::new(section.heading)
            .num_columns(2)
            .spacing([14.0, 3.0])
            .show(ui, |ui| {
                for (label, value) in &section.rows {
                    ui.label(RichText::new(*label).color(MUTED));
                    ui.label(value);
                    ui.end_row();
                }
            });
    }
    if !card.notes.is_empty() {
        ui.add_space(6.0);
        ui.separator();
        for note in &card.notes {
            ui.label(note);
        }
    }
}

fn selected_card(targets: &SimulationTargets) -> Option<Card> {
    let result = targets.view.result()?;
    match targets.selection.0? {
        Target::Star => {
            let star = targets.view.star()?;
            Some(cards::star_card(
                star,
                result.snapshot.habitable_zones.first(),
                result.snapshot.bodies.len(),
            ))
        }
        Target::Body(id) => {
            let (index, body) = result
                .snapshot
                .bodies
                .iter()
                .enumerate()
                .find(|(_, b)| b.id == id)?;
            Some(cards::body_card(body, index + 1))
        }
    }
}

pub fn draw(
    mut contexts: EguiContexts,
    mut state: ResMut<AppState>,
    mut targets: SimulationTargets,
    mut captured: ResMut<PointerCaptured>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if !state.show_ui {
        captured.0 = false;
        return Ok(());
    }

    egui::Window::new("StarGen-NG")
        .anchor(Align2::LEFT_TOP, [12.0, 12.0])
        .default_width(330.0)
        .resizable(false)
        .show(ctx, |ui| controls(ui, &mut state, &mut targets));

    egui::Window::new("Formation")
        .anchor(Align2::CENTER_BOTTOM, [0.0, -12.0])
        .resizable(false)
        .collapsible(false)
        .title_bar(false)
        .show(ctx, |ui| timeline(ui, &mut targets));

    if let Some(data) = selected_card(&targets) {
        let mut open = true;
        egui::Window::new(data.title.clone())
            .id(egui::Id::new("data-card"))
            .anchor(Align2::RIGHT_TOP, [-12.0, 12.0])
            .default_width(300.0)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| card(ui, &data));
        if !open {
            targets.selection.0 = None;
        }
    }

    captured.0 = ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui();
    Ok(())
}
