//! JSON round-trip and compatibility tests for the data contracts.

use sg_schema::*;

fn sample_body() -> Body {
    Body {
        id: BodyId(3),
        name: "Velora d".into(),
        designation: "Velora b".into(),
        kind: BodyKind::Planet,
        host: HostRef::Star(StarId(0)),
        orbit: Orbit {
            a_au: 1.0,
            e: 0.0167,
            i_deg: 0.0,
            long_asc_node_deg: 348.7,
            arg_periapsis_deg: 114.2,
            mean_anomaly_deg: 357.5,
            period_days: 365.256,
        },
        mass_mearth: 1.0,
        radius_km: 6378.0,
        density_g_cm3: 5.52,
        surface_gravity_gees: Some(1.0),
        escape_velocity_km_s: 11.2,
        albedo: 0.3,
        equilibrium_temp_k: 255.0,
        minerals: vec![MineralShare {
            mineral: Mineral::Silicates,
            share: 1.0,
        }],
        composition: Composition {
            metal_frac: 0.32,
            rock_frac: 0.68,
            water_frac: 0.0,
            volatiles_frac: 0.0,
            h_he_frac: 0.0,
        },
        rotation: Rotation {
            period_hours: Some(23.93),
            obliquity_deg: None,
            tidally_locked: false,
            spin_orbit_resonance: false,
        },
        atmosphere: Some(Atmosphere {
            surface_pressure_mbar: Some(1013.25),
            min_molecular_weight: 6.0,
            gases: vec![
                GasFraction {
                    symbol: "N".into(),
                    partial_pressure_mbar: 791.0,
                },
                GasFraction {
                    symbol: "O".into(),
                    partial_pressure_mbar: 212.0,
                },
            ],
        }),
        climate: Some(Climate {
            surface_temp_k: 288.0,
            day_temp_k: 293.0,
            night_temp_k: 283.0,
            max_temp_k: 320.0,
            min_temp_k: 240.0,
            greenhouse_rise_k: 33.0,
            runaway_greenhouse: false,
            water_boiling_point_k: Some(373.0),
            ocean_frac: 0.71,
            ice_frac: 0.03,
            cloud_frac: 0.5,
        }),
        class: PlanetClass::Terrestrial,
        habitability: Habitability {
            in_conservative_hz: true,
            in_optimistic_hz: true,
            surface_liquid_water: true,
        },
        rings: None,
        visual: VisualDescriptor {
            texture_seed: u64::MAX,
            ocean_frac: 0.71,
            ice_frac: 0.03,
            cloud_frac: 0.5,
        },
        formation: FormationSummary {
            origin_a_au: 1.1,
            mergers: 4,
            captured_gas: false,
        },
    }
}

fn sample_snapshot() -> SystemSnapshot {
    SystemSnapshot {
        schema_version: SCHEMA_VERSION.into(),
        model_version: "0.1.0".into(),
        params_hash: "0123456789abcdef".into(),
        age_gyr: 4.6,
        hierarchy: HierarchyNode::Pair {
            id: NodeId(2),
            a: Box::new(HierarchyNode::Star(Star {
                id: StarId(0),
                name: "Velora A".into(),
                mass_msun: 1.0,
                luminosity_lsun: 1.0,
                radius_rsun: 1.0,
                teff_k: 5772.0,
                spectral_class: "G2".into(),
                color_linear_rgb: [1.0, 0.93, 0.87],
                age_gyr: 4.6,
                main_sequence_lifetime_gyr: 10.0,
                hydrogen_frac: 0.7,
                helium_frac: 0.28,
                metal_frac: 0.02,
            })),
            b: Box::new(HierarchyNode::Star(Star {
                id: StarId(1),
                name: "Velora B".into(),
                mass_msun: 0.5,
                luminosity_lsun: 0.06,
                radius_rsun: 0.57,
                teff_k: 3800.0,
                spectral_class: "K9".into(),
                color_linear_rgb: [1.0, 0.6, 0.3],
                age_gyr: 4.6,
                main_sequence_lifetime_gyr: 80.0,
                hydrogen_frac: 0.7,
                helium_frac: 0.28,
                metal_frac: 0.02,
            })),
            orbit: Orbit {
                a_au: 80.0,
                e: 0.4,
                i_deg: 12.0,
                long_asc_node_deg: 10.0,
                arg_periapsis_deg: 20.0,
                mean_anomaly_deg: 30.0,
                period_days: 213_000.0,
            },
        },
        bodies: vec![sample_body()],
        belts: vec![Belt {
            name: "Velora Belt".into(),
            host: HostRef::Star(StarId(0)),
            inner_au: 2.1,
            outer_au: 3.3,
            mass_mearth: 4.0e-4,
        }],
        comets: vec![Comet {
            id: CometId(0),
            name: "Calira".into(),
            designation: "1P/Calira".into(),
            host: HostRef::Star(StarId(0)),
            family: CometFamily::JupiterFamily,
            orbit: Orbit {
                a_au: 3.5,
                e: 0.64,
                i_deg: 7.0,
                long_asc_node_deg: 1.0,
                arg_periapsis_deg: 2.0,
                mean_anomaly_deg: 3.0,
                period_days: 2400.0,
            },
            diameter_km: 4.0,
        }],
        comet_reservoirs: vec![CometReservoir {
            host: HostRef::Star(StarId(0)),
            belt_mass_mearth: 0.02,
            ejectors: 2,
            active_comets: 300.0,
            oort_mass_mearth: None,
        }],
        disks_at_dispersal: vec![DiskSummary {
            id: DiskId(0),
            host: HostRef::Barycenter(NodeId(2)),
            inner_au: 0.05,
            outer_au: 30.0,
            lifetime_myr: 3.2,
            initial_mass_msun: 0.05,
            dust_to_gas: 0.0149,
            final_ice_line_au: 1.1,
            characteristic_radius_au: 35.0,
            viscous_time_myr: 0.4,
        }],
        habitable_zones: vec![HabitableZone {
            host: HostRef::Star(StarId(0)),
            optimistic_inner_au: 0.75,
            conservative_inner_au: 0.95,
            conservative_outer_au: 1.68,
            optimistic_outer_au: 1.77,
        }],
        planet_zones: vec![PlanetZone {
            host: HostRef::Barycenter(NodeId(0)),
            inner_au: 0.65,
            outer_au: 48.0,
            limited_inside: true,
            limited_outside: false,
        }],
    }
}

fn sample_timeline() -> EventTimeline {
    let b = BodyId(3);
    let d = DiskId(0);
    let kinds = vec![
        EventKind::DiskFormed { disk: d },
        EventKind::IceLineMoved {
            disk: d,
            species: Species::H2o,
            r_au: 2.7,
        },
        EventKind::PlanetesimalsFormed {
            disk: d,
            r_au: 2.7,
            mass_mearth: 3.0,
        },
        EventKind::EmbryoInjected {
            body: b,
            host: HostRef::Star(StarId(0)),
            a_au: 1.1,
            e: 0.05,
        },
        EventKind::DustSwept {
            body: b,
            dm_mearth: 0.4,
            inner_au: 0.9,
            outer_au: 1.3,
        },
        EventKind::PebbleIsolationReached { body: b },
        EventKind::RunawayGasAccretion { body: b },
        EventKind::GapOpened { body: b },
        EventKind::Migrated {
            body: b,
            from_au: 1.1,
            to_au: 1.0,
        },
        EventKind::ResonanceCaptured {
            a: b,
            b: BodyId(4),
            ratio: (3, 2),
        },
        EventKind::Collision {
            a: b,
            b: BodyId(5),
            result: b,
        },
        EventKind::Ejected { body: BodyId(6) },
        EventKind::MoonFormed {
            moon: BodyId(7),
            host: b,
            mechanism: MoonOrigin::GiantImpact,
        },
        EventKind::DiskDispersed { disk: d },
        EventKind::AtmosphereStripped {
            body: b,
            fraction: 0.25,
        },
        EventKind::TidallyLocked { body: b },
    ];
    EventTimeline {
        events: kinds
            .into_iter()
            .enumerate()
            .map(|(i, kind)| Event {
                t_myr: i as f64 * 0.5,
                kind,
            })
            .collect(),
        keyframes: vec![Keyframe {
            t_myr: 1.0,
            bodies: vec![KeyframeBody {
                id: b,
                host: HostRef::Barycenter(NodeId(0)),
                a_au: 1.1,
                e: 0.05,
                mass_mearth: 0.4,
            }],
        }],
    }
}

#[test]
fn snapshot_round_trips() {
    let snapshot = sample_snapshot();
    let json = serde_json::to_string_pretty(&snapshot).unwrap();
    let back: SystemSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snapshot);
}

#[test]
fn timeline_round_trips_with_every_event_kind() {
    let timeline = sample_timeline();
    let json = serde_json::to_string(&timeline).unwrap();
    let back: EventTimeline = serde_json::from_str(&json).unwrap();
    assert_eq!(back, timeline);
}

#[test]
fn params_round_trip() {
    let params = SimulationParams {
        seed: u64::MAX,
        preset: Some(PresetId::Circumbinary),
        system: SystemSpec::Explicit(StarNode::Pair {
            a: Box::new(StarNode::Star(StarSpec { mass_msun: 0.69 })),
            b: Box::new(StarNode::Star(StarSpec { mass_msun: 0.20 })),
            orbit: OrbitSpec {
                a_au: 0.22,
                e: 0.16,
            },
        }),
        age_gyr: Some(3.0),
        metallicity_feh: Some(-0.3),
        ..SimulationParams::default()
    };
    let json = serde_json::to_string(&params).unwrap();
    let back: SimulationParams = serde_json::from_str(&json).unwrap();
    assert_eq!(back, params);
}

/// A user can write only the fields they care about.
#[test]
fn params_fill_in_defaults() {
    let params: SimulationParams =
        serde_json::from_str(r#"{"seed": 42, "formation": {"classic": {"gas_dust_ratio": 80}}}"#)
            .unwrap();
    assert_eq!(params.seed, 42);
    assert_eq!(params.schema_version, SCHEMA_VERSION);
    assert_eq!(params.fidelity, Fidelity::Classic);
    assert_eq!(params.formation.classic.gas_dust_ratio, 80.0);
    assert_eq!(params.formation.classic.dust_density_coeff, 1.5e-3);
    assert!(params.toggles.atmospheres);
}

/// Field and variant names are part of the contract: units in names,
/// snake_case enums.
#[test]
fn params_reject_unknown_fields() {
    // A misplaced field must not silently leave the default in force.
    let misplaced = r#"{"formation":{"bump_trap_fraction":0.3}}"#;
    assert!(serde_json::from_str::<SimulationParams>(misplaced).is_err());
    let misspelt = r#"{"disk":{"mass_fraction_medain":0.01}}"#;
    assert!(serde_json::from_str::<SimulationParams>(misspelt).is_err());
    let placed = r#"{"formation":{"standard":{"bump_trap_fraction":0.3}}}"#;
    assert!(serde_json::from_str::<SimulationParams>(placed).is_ok());
}

#[test]
fn wire_names_are_stable() {
    let json = serde_json::to_value(sample_snapshot()).unwrap();
    let body = &json["bodies"][0];
    assert_eq!(body["orbit"]["a_au"], 1.0);
    assert_eq!(body["mass_mearth"], 1.0);
    assert_eq!(body["class"], "terrestrial");
    assert_eq!(body["kind"], "planet");
    assert_eq!(body["host"]["star"], 0);
    assert_eq!(body["climate"]["surface_temp_k"], 288.0);
    assert_eq!(json["hierarchy"]["pair"]["a"]["star"]["teff_k"], 5772.0);
    let params = serde_json::to_value(SimulationParams::default()).unwrap();
    assert_eq!(params["fidelity"], "classic");
    assert_eq!(params["system"]["auto"]["multiplicity"], "single");
    let timeline = serde_json::to_value(sample_timeline()).unwrap();
    assert_eq!(
        timeline["events"][3]["kind"]["embryo_injected"]["a_au"],
        1.1
    );
    assert_eq!(timeline["events"][3]["t_myr"], 1.5);
}

/// The Classic defaults are Dole's Set 4 (1969, Table 2).
#[test]
fn classic_defaults_are_doles_set_4() {
    let c = ClassicParams::default();
    assert_eq!(
        (
            c.dust_density_coeff,
            c.gas_dust_ratio,
            c.crit_mass_coeff_msun,
            c.alpha,
            c.n
        ),
        (1.5e-3, 50.0, 1.2e-5, 5.0, 3.0)
    );
    assert_eq!(
        (
            c.eccentricity_coeff,
            c.cloud_eccentricity,
            c.protoplanet_mass_msun
        ),
        (0.077, 0.25, 1.0e-15)
    );
}
