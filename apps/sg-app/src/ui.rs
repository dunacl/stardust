//! The egui layer: controls, formation timeline, data card, and the name
//! labels beside the star and planets.

use crate::cards::{self, Card, Companion};
use crate::state::{run_simulation, AppState, SimulationTargets, DEFAULT_PLAYBACK_SECS};
use bevy::prelude::*;
use bevy_egui::egui::{self, Align2, Color32, RichText};
use bevy_egui::EguiContexts;
use sg_render::scene::system_extent;
use sg_render::{layout, BodyPlacements, PointerCaptured, Quality, ScaleMode, Target};
use sg_schema::{Fidelity, HostRef, PresetId, StarNode, SystemSpec};
use sg_sim::{preset_availability, PresetAvailability, StellarModel, ALL_PRESETS, MODEL_VERSION};

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

fn mass_slider(ui: &mut egui::Ui, mass: &mut f64, label: &str) {
    ui.add(
        egui::Slider::new(mass, 0.1..=3.0)
            .logarithmic(true)
            .text(label)
            .fixed_decimals(2),
    );
}

/// Sliders for an explicit hierarchy: a mass per star, a separation and an
/// eccentricity per pair. `next` numbers the stars in the order shown.
fn hierarchy_controls(ui: &mut egui::Ui, node: &mut StarNode, next: &mut u32) {
    match node {
        StarNode::Star(spec) => {
            *next += 1;
            mass_slider(ui, &mut spec.mass_msun, &format!("Star {next} mass (M☉)"));
        }
        StarNode::Pair { a, b, orbit } => {
            ui.push_id(*next, |ui| {
                hierarchy_controls(ui, a, next);
                hierarchy_controls(ui, b, next);
                ui.add(
                    egui::Slider::new(&mut orbit.a_au, 0.03..=500.0)
                        .logarithmic(true)
                        .text("Separation (AU)"),
                );
                ui.add(
                    egui::Slider::new(&mut orbit.e, 0.0..=0.8)
                        .text("Eccentricity")
                        .fixed_decimals(2),
                );
            });
        }
    }
}

fn system_controls(ui: &mut egui::Ui, system: &mut SystemSpec) {
    match system {
        SystemSpec::Auto {
            primary_mass_msun: Some(mass),
            ..
        } => {
            mass_slider(ui, mass, "Star mass (M☉)");
            if !(0.2..=1.5).contains(mass) {
                ui.label(
                    RichText::new("Outside StarGen's 0.2–1.5 M☉: the star is evolved instead.")
                        .small()
                        .color(MUTED),
                );
            }
        }
        SystemSpec::Auto {
            primary_mass_msun: None,
            multiplicity: None,
        } => {
            ui.label(
                RichText::new("Stars drawn from the mass function and multiplicity statistics")
                    .color(MUTED),
            );
        }
        SystemSpec::Auto { .. } => {
            ui.label(RichText::new("Star mass: drawn at random (0.7–1.4 M☉)").color(MUTED));
        }
        SystemSpec::Explicit(node) => {
            hierarchy_controls(ui, node, &mut 0);
        }
    }
}

fn controls(ui: &mut egui::Ui, state: &mut AppState, targets: &mut SimulationTargets) {
    let mut fidelity = state.fidelity;
    ui.horizontal(|ui| {
        ui.radio_value(&mut fidelity, Fidelity::Standard, "Standard")
            .on_hover_text(
                "The modern model: a gas disk that evolves and clears, cores grown by pebble \
                 accretion, gas giants by runaway accretion, migration, and loss of atmospheres \
                 to the star's radiation.",
            );
        ui.radio_value(&mut fidelity, Fidelity::Classic, "Classic")
            .on_hover_text(
                "The Dole/StarGen accretion model of 1970–2000. A single star of 0.2–1.5 M☉ \
                 is modelled exactly as StarGen does; other stars, and all multiple systems, \
                 use modern stars and orbital-stability limits around it.",
            );
        ui.radio_value(&mut fidelity, Fidelity::High, "High")
            .on_hover_text(
                "The Standard model, with the orbits of the massive planets followed directly \
                 for up to a million years after the gas clears. Giants packed too closely \
                 throw one another onto elongated orbits or out of the system, where the \
                 Standard tier would merge them. Takes a few seconds when there are two or \
                 more massive planets; otherwise it is the Standard tier.",
            );
        ui.label(RichText::new(format!("model {MODEL_VERSION}")).color(MUTED));
    });
    if fidelity != state.fidelity {
        state.set_fidelity(fidelity);
    }
    if let Some(result) = targets.view.result() {
        let shown = match (
            result.diagnostics.fidelity,
            result.diagnostics.stellar_model,
        ) {
            (Fidelity::Classic, StellarModel::Fogg1985) => "Shown: Classic, as StarGen",
            (Fidelity::Classic, _) => "Shown: Classic accretion, evolved stars",
            (Fidelity::High, _) => "Shown: High",
            _ => "Shown: Standard",
        };
        ui.label(RichText::new(shown).small().color(MUTED));
    }
    ui.separator();

    egui::ComboBox::from_label("Preset")
        .selected_text(preset_name(state.preset))
        .show_ui(ui, |ui| {
            for preset in ALL_PRESETS {
                match preset_availability(preset, state.fidelity) {
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

    system_controls(ui, &mut state.params.system);

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
            ui.label(RichText::new("sampled").color(MUTED));
        }
    });

    if state.fidelity != Fidelity::Classic {
        standard_controls(ui, state);
    } else {
        classic_controls(ui, state);
    }

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
    } else if let Some(ms) = state.last_simulation_ms {
        // The time this device took: the budget is two seconds on a phone
        // (PLAN.md §4).
        ui.label(
            RichText::new(format!("Simulated in {}", simulation_time(ms)))
                .small()
                .color(MUTED),
        );
    }

    view_controls(ui, targets);
}

/// A simulation's duration for display: milliseconds below a second.
pub fn simulation_time(ms: f64) -> String {
    if ms < 1000.0 {
        format!("{ms:.0} ms")
    } else {
        format!("{:.1} s", ms / 1000.0)
    }
}

/// A value the model draws unless the box is ticked, in which case the
/// slider sets it.
fn optional_slider(
    ui: &mut egui::Ui,
    value: &mut Option<f64>,
    label: &str,
    when_fixed: f64,
    slider: impl FnOnce(&mut f64) -> egui::Slider<'_>,
) {
    let mut fixed = value.is_some();
    ui.horizontal(|ui| {
        if ui.checkbox(&mut fixed, label).changed() {
            *value = fixed.then_some(when_fixed);
        }
        match value.as_mut() {
            Some(v) => {
                ui.add(slider(v));
            }
            None => {
                ui.label(RichText::new("drawn at random").color(MUTED));
            }
        }
    });
}

fn standard_controls(ui: &mut egui::Ui, state: &mut AppState) {
    ui.collapsing("Disk and formation", |ui| {
        optional_slider(
            ui,
            &mut state.params.metallicity_feh,
            "Metallicity",
            0.0,
            |v| {
                egui::Slider::new(v, -0.6..=0.5)
                    .suffix(" dex")
                    .fixed_decimals(2)
            },
        );
        optional_slider(
            ui,
            &mut state.params.disk.mass_fraction,
            "Disk mass",
            0.03,
            |v| {
                egui::Slider::new(v, 0.003..=0.2)
                    .logarithmic(true)
                    .suffix(" × star")
            },
        );
        optional_slider(
            ui,
            &mut state.params.disk.lifetime_myr,
            "Disk lifetime",
            3.0,
            |v| {
                egui::Slider::new(v, 0.5..=10.0)
                    .suffix(" Myr")
                    .fixed_decimals(1)
            },
        );
        ui.checkbox(&mut state.params.toggles.migration, "Migration")
            .on_hover_text("Planets drift inward through the gas disk as they grow.");
        if ui.button("Reset").clicked() {
            state.params.metallicity_feh = None;
            state.params.disk = Default::default();
            state.params.formation.standard = Default::default();
            state.params.toggles.migration = true;
        }
    });
}

fn classic_controls(ui: &mut egui::Ui, state: &mut AppState) {
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
}

fn view_controls(ui: &mut egui::Ui, targets: &mut SimulationTargets) {
    ui.separator();
    ui.label(RichText::new("View").strong());
    let mut mode = targets.settings.scale.to;
    ui.horizontal_wrapped(|ui| {
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
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut targets.settings.show_orbits, "Orbits");
        ui.checkbox(&mut targets.settings.show_habitable_zone, "Habitable zone");
        ui.checkbox(&mut targets.settings.show_stability_limits, "Limits")
            .on_hover_text(
                "Red rings: where a companion star's pull ends the zone in which \
                 planets keep stable orbits.",
            );
        ui.checkbox(&mut targets.playback.orbits_running, "Orbital motion");
    });
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut targets.settings.show_labels, "Names");
        ui.checkbox(&mut targets.settings.show_disk, "Disk, while forming")
            .on_hover_text(
                "The gas and dust the planets grow in. How dense it is, where giants have \
                 opened gaps and when it clears come from the model; the streaks do not.",
            );
        ui.checkbox(&mut targets.settings.show_backdrop, "Sky backdrop")
            .on_hover_text("Scenery: the stars and galaxies behind the system are not simulated.");
    });
    ui.horizontal(|ui| {
        ui.label("Effects");
        for quality in Quality::ALL {
            ui.radio_value(&mut targets.settings.quality, quality, quality.label());
        }
    })
    .response
    .on_hover_text(
        "Reduced is for phones and older graphics: no glow around stars, coarser \
         spheres, fewer belt rocks, less surface detail.",
    );
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
        RichText::new(
            "Drag to rotate · scroll or pinch to zoom · click or tap a body for its card",
        )
        .small()
        .color(MUTED),
    );
}

fn timeline(ui: &mut egui::Ui, targets: &mut SimulationTargets, narrow: bool) {
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

    let buttons = |ui: &mut egui::Ui, targets: &mut SimulationTargets| {
        let at_end = targets.playback.t_myr >= end;
        let label = if targets.playback.playing {
            "Pause"
        } else {
            "Play"
        };
        let height = ui.spacing().interact_size.y.max(22.0);
        if ui
            .add_sized([56.0, height], egui::Button::new(label))
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
    };
    let time_slider = |ui: &mut egui::Ui, targets: &mut SimulationTargets, width: f32| {
        ui.spacing_mut().slider_width = width;
        // The slider works on a copy: it rounds what it shows, and writing
        // that back would nudge the time off the end of the timeline.
        let mut shown = targets.playback.t_myr;
        let slider = egui::Slider::new(&mut shown, 0.0..=end)
            .suffix(" Myr")
            .fixed_decimals(2);
        let response = ui.add(slider);
        if response.dragged() || (response.changed() && response.has_focus()) {
            targets.playback.t_myr = shown.clamp(0.0, end);
            targets.playback.playing = false;
        }
    };
    let speed_slider = |ui: &mut egui::Ui, targets: &mut SimulationTargets, width: f32| {
        // Speed as a multiple of the default playback duration.
        let base = end / DEFAULT_PLAYBACK_SECS;
        let mut speed = targets.playback.myr_per_second / base;
        ui.spacing_mut().slider_width = width;
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
    };
    if narrow {
        // Two rows: a phone is too narrow for buttons and both sliders.
        // The value box beside a slider takes about 90 points.
        let room = ui.available_width();
        ui.horizontal(|ui| {
            buttons(ui, targets);
            let left = (room - ui.min_rect().width() - 110.0).max(40.0);
            speed_slider(ui, targets, left);
        });
        ui.horizontal(|ui| time_slider(ui, targets, (room - 100.0).max(80.0)));
    } else {
        ui.horizontal(|ui| {
            buttons(ui, targets);
            time_slider(ui, targets, 420.0);
            speed_slider(ui, targets, 90.0);
        });
    }
    let clock = match targets.view.result().map(|r| r.diagnostics.fidelity) {
        Some(Fidelity::Classic) => "Classic times are nominal, not physical",
        _ => "time since the disk formed",
    };
    let phase = if targets.playback.t_myr >= end {
        "finished system".to_string()
    } else {
        format!("{bodies_now} embryos")
    };
    ui.label(
        RichText::new(format!("{phase} · {events} formation events · {clock}"))
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
    let snapshot = &result.snapshot;
    match targets.selection.0? {
        Target::Star(id) => {
            let star = targets.view.star(id)?;
            let host = HostRef::Star(id);
            let companion = layout::parent_orbit(&snapshot.hierarchy, host).map(
                |(pair, orbit, _own_mass, sibling_mass)| {
                    // Its companions: the other stars under the same pair.
                    let names: Vec<&str> =
                        layout::find_host(&snapshot.hierarchy, HostRef::Barycenter(pair))
                            .map(layout::stars)
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|other| other.id != id)
                            .map(|other| other.name.as_str())
                            .collect();
                    Companion {
                        name: names.join(" + "),
                        separation_au: orbit.a_au,
                        eccentricity: orbit.e,
                        period_days: orbit.period_days,
                        mass_msun: sibling_mass,
                    }
                },
            );
            Some(cards::star_card(
                star,
                snapshot.habitable_zones.iter().find(|hz| hz.host == host),
                snapshot.planet_zones.iter().find(|zone| zone.host == host),
                snapshot.bodies.iter().filter(|b| b.host == host).count(),
                companion.as_ref(),
                snapshot.disks_at_dispersal.iter().find(|d| d.host == host),
            ))
        }
        Target::Body(id) => {
            let body = snapshot.bodies.iter().find(|b| b.id == id)?;
            // Counted outward among the planets of the same host.
            let number = snapshot
                .bodies
                .iter()
                .filter(|b| b.host == body.host && b.orbit.a_au <= body.orbit.a_au)
                .count();
            Some(cards::body_card(body, number))
        }
    }
}

/// A name to draw: where, what, and how much it matters when names collide.
struct Label {
    anchor: egui::Pos2,
    text: String,
    selected: bool,
    mass_mearth: f64,
}

const LABEL_FONT_SIZE: f32 = 13.0;

/// Names beside the star and the finished planets, under the panels. Where
/// two would overlap, the selected body wins, then the heavier one.
fn labels(
    ctx: &egui::Context,
    targets: &SimulationTargets,
    placements: &BodyPlacements,
    camera: &Camera,
    camera_transform: &GlobalTransform,
) {
    let Some(result) = targets.view.result() else {
        return;
    };
    // Embryos have no names: only the finished system is labelled.
    let finished = targets.playback.t_myr >= targets.view.timeline_end_myr();
    let eye = camera_transform.translation();

    let mut wanted: Vec<Label> = Vec::new();
    for (target, placement) in placements.iter() {
        let (text, mass_mearth) = match target {
            Target::Star(id) => match targets.view.star(id) {
                Some(star) => (star.name.clone(), f64::INFINITY),
                None => continue,
            },
            Target::Body(id) => match result.snapshot.bodies.iter().find(|b| b.id == id) {
                Some(body) if finished => (body.name.clone(), body.mass_mearth),
                _ => continue,
            },
        };
        // Anchor at the body's upper-right edge as seen on screen.
        let edge = placement.position
            + (*camera_transform.right() + *camera_transform.up()) * (placement.radius * 0.75);
        if (edge - eye).dot(*camera_transform.forward()) <= 0.0 {
            continue;
        }
        let Ok(at) = camera.world_to_viewport(camera_transform, edge) else {
            continue;
        };
        wanted.push(Label {
            anchor: egui::pos2(at.x + 4.0, at.y - 4.0),
            text,
            selected: targets.selection.0 == Some(target),
            mass_mearth,
        });
    }
    wanted.sort_by(|a, b| {
        b.selected
            .cmp(&a.selected)
            .then(b.mass_mearth.total_cmp(&a.mass_mearth))
    });

    let painter = ctx.layer_painter(egui::LayerId::background());
    let font = egui::FontId::proportional(LABEL_FONT_SIZE);
    let mut taken: Vec<egui::Rect> = Vec::new();
    for label in wanted {
        let color = if label.selected {
            Color32::from_rgb(255, 235, 140)
        } else {
            Color32::from_rgba_unmultiplied(205, 214, 230, 215)
        };
        let galley = painter.layout_no_wrap(label.text, font.clone(), color);
        let rect = egui::Rect::from_min_size(
            label.anchor - egui::vec2(0.0, galley.size().y),
            galley.size(),
        );
        if taken.iter().any(|other| other.intersects(rect)) {
            continue;
        }
        // A dark copy underneath keeps the text readable over bright things.
        let shadow = painter.layout_no_wrap(
            galley.text().to_string(),
            font.clone(),
            Color32::from_black_alpha(200),
        );
        painter.galley(rect.min + egui::vec2(1.0, 1.0), shadow, Color32::BLACK);
        painter.galley(rect.min, galley, color);
        taken.push(rect.expand(2.0));
    }
}

/// Screens narrower than this, in points, get the phone layout: the
/// controls in a drawer, the playback bar across the bottom, the data card
/// as a sheet above it. A phone held upright is 360–430 points wide; a
/// small tablet or a phone on its side is wider and has room for the
/// floating panels.
pub const NARROW_BELOW_POINTS: f32 = 700.0;

pub fn is_narrow(width_points: f32) -> bool {
    width_points < NARROW_BELOW_POINTS
}

/// Height of a control a finger can hit, points: Apple's guidelines ask
/// for 44, Android's for 48; egui's default of 18 is for a mouse.
const TOUCH_TARGET_POINTS: f32 = 40.0;

/// Makes controls finger-sized on a narrow screen, and restores the
/// defaults on a wide one.
fn apply_touch_sizes(ctx: &egui::Context, narrow: bool) {
    ctx.all_styles_mut(|style| {
        let defaults = egui::style::Spacing::default();
        if narrow {
            style.spacing.interact_size.y = TOUCH_TARGET_POINTS;
            style.spacing.button_padding = egui::vec2(12.0, 8.0);
            style.spacing.item_spacing = egui::vec2(10.0, 8.0);
            style.spacing.icon_width = 22.0;
            style.spacing.slider_rail_height = 12.0;
            style.spacing.combo_height = 320.0;
        } else {
            style.spacing.interact_size = defaults.interact_size;
            style.spacing.button_padding = defaults.button_padding;
            style.spacing.item_spacing = defaults.item_spacing;
            style.spacing.icon_width = defaults.icon_width;
            style.spacing.slider_rail_height = defaults.slider_rail_height;
            style.spacing.combo_height = defaults.combo_height;
        }
    });
}

/// The phone layout.
fn narrow_layout(
    ctx: &egui::Context,
    state: &mut AppState,
    targets: &mut SimulationTargets,
    screen: egui::Rect,
) {
    let open = *state.panel_open.get_or_insert(false);
    let margin = 8.0;
    let width = screen.width() - 2.0 * margin;

    // The playback bar across the bottom.
    let bar = egui::Window::new("Formation")
        .anchor(Align2::CENTER_BOTTOM, [0.0, -margin])
        .fixed_size([width - 16.0, 0.0])
        .resizable(false)
        .collapsible(false)
        .title_bar(false)
        .show(ctx, |ui| timeline(ui, targets, true));
    let bar_top = bar.map_or(screen.bottom(), |shown| shown.response.rect.top());

    if open {
        // The controls as a drawer down the left side.
        let mut keep = true;
        egui::Window::new("Stardust")
            .id(egui::Id::new("controls-drawer"))
            .anchor(Align2::LEFT_TOP, [margin, margin])
            .fixed_size([
                (width - 16.0).min(360.0),
                (bar_top - screen.top() - 2.0 * margin - 48.0).max(160.0),
            ])
            .resizable(false)
            .collapsible(false)
            .open(&mut keep)
            .vscroll(true)
            .show(ctx, |ui| controls(ui, state, targets));
        if !keep {
            state.panel_open = Some(false);
        }
        return;
    }
    egui::Area::new(egui::Id::new("controls-button"))
        .anchor(Align2::LEFT_TOP, [margin, margin])
        .show(ctx, |ui| {
            if ui.button("☰ Controls").clicked() {
                state.panel_open = Some(true);
            }
        });

    // The card as a sheet just above the bar; it gives way to the drawer.
    if let Some(data) = selected_card(targets) {
        let mut keep = true;
        egui::Window::new(data.title.clone())
            .id(egui::Id::new("data-card-sheet"))
            .pivot(Align2::CENTER_BOTTOM)
            .fixed_pos([screen.center().x, bar_top - margin])
            .fixed_size([width - 16.0, 0.30 * screen.height()])
            .resizable(false)
            .open(&mut keep)
            .vscroll(true)
            .show(ctx, |ui| card(ui, &data));
        if !keep {
            targets.selection.0 = None;
        }
    }
}

pub fn draw(
    mut contexts: EguiContexts,
    mut state: ResMut<AppState>,
    mut targets: SimulationTargets,
    mut captured: ResMut<PointerCaptured>,
    placements: Res<BodyPlacements>,
    camera: Single<(&Camera, &GlobalTransform), With<Camera3d>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if targets.settings.show_labels {
        let (camera, camera_transform) = *camera;
        labels(ctx, &targets, &placements, camera, camera_transform);
    }
    if !state.show_ui {
        captured.0 = false;
        return Ok(());
    }

    let screen = ctx.content_rect();
    let narrow = is_narrow(screen.width());
    apply_touch_sizes(ctx, narrow);
    if narrow {
        narrow_layout(ctx, &mut state, &mut targets, screen);
    } else {
        egui::Window::new("Stardust")
            .anchor(Align2::LEFT_TOP, [12.0, 12.0])
            .default_width(330.0)
            .resizable(false)
            .show(ctx, |ui| controls(ui, &mut state, &mut targets));

        egui::Window::new("Formation")
            .anchor(Align2::CENTER_BOTTOM, [0.0, -12.0])
            .resizable(false)
            .collapsible(false)
            .title_bar(false)
            .show(ctx, |ui| timeline(ui, &mut targets, false));

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
    }

    captured.0 = ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phones_upright_are_narrow_and_tablets_are_not() {
        // iPhone and common Android widths, points.
        for width in [360.0, 390.0, 430.0] {
            assert!(is_narrow(width));
        }
        // A phone on its side, a tablet, a desktop window.
        for width in [844.0, 768.0, 1440.0] {
            assert!(!is_narrow(width));
        }
    }

    #[test]
    fn simulation_times_read_naturally() {
        assert_eq!(simulation_time(0.4), "0 ms");
        assert_eq!(simulation_time(76.5), "76 ms");
        assert_eq!(simulation_time(999.4), "999 ms");
        assert_eq!(simulation_time(1840.0), "1.8 s");
    }
}
