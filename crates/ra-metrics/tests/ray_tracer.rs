//! The ray tracer against the diffuse-field formulas in a shoebox. With the same
//! absorption on every wall and scattering on, its decay should follow Eyring.

use ra_core::{AirConditions, BandSet, Group, Material, SceneMeta};
use ra_engine::{Point, PreparedScene, RunRequest, Solver, rt::RayTracer};
use ra_metrics::{Options, from_energy, statistical::reverberation};

fn shoebox(alpha: f32) -> PreparedScene {
    let meta = SceneMeta {
        origin: [0.0; 3],
        groups: vec![Group {
            key: "Room".into(),
            layer_path: "Room".into(),
            material: None,
            label: "Room".into(),
            acoustic_material: Some("M".into()),
        }],
        importer: "test".into(),
    };
    let m = Material {
        name: "M".into(),
        absorption: vec![alpha; 8],
        scattering: vec![0.5; 8],
        source: String::new(),
    };
    PreparedScene::new(
        meta,
        ra_geom::shoebox([10.0, 8.0, 4.0]),
        vec![m],
        vec![Point { id: "S".into(), position: [2.5, 3.0, 1.5] }],
        vec![Point { id: "R".into(), position: [7.0, 5.0, 1.2] }],
    )
}

fn t30_at_1k(alpha: f32) -> (f64, f64) {
    let scene = shoebox(alpha);
    let run = RunRequest {
        solver: "rt".into(),
        bands: BandSet::Octave63To8k,
        max_order: 0,
        n_rays: 20_000,
        seed: 1,
        air_absorption_per_m: None,
        air: AirConditions::default(),
        receiver_radius_m: 0.5,
        duration_s: 2.0,
        receivers: vec!["R".into()],
        grids: vec![],
        listening: vec![],
    };
    let rt = RayTracer;
    let parts = rt.plan(&scene, &run).iter().map(|c| rt.run_chunk(&scene, &run, c)).collect();
    let out = rt.merge(&run, parts);
    let m = from_energy(&out.receivers[0], 1000.0, &Options::default()).unwrap();
    let air = AirConditions::default();
    let eyring = reverberation(
        320.0,
        304.0,
        304.0 * f64::from(alpha),
        air.energy_attenuation_per_m(1000.0),
        air.speed_of_sound(),
    )
    .eyring_s;
    (f64::from(m.t30_s[4].unwrap()), eyring)
}

#[test]
fn shoebox_decay_follows_eyring() {
    for alpha in [0.1, 0.3] {
        let (t30, eyring) = t30_at_1k(alpha);
        assert!((t30 / eyring - 1.0).abs() < 0.1, "alpha {alpha}: T30 {t30:.3} s, Eyring {eyring:.3} s");
    }
}
