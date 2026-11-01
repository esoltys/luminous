use luminous_equalizer::{parse_parametric_profile, read_profile_file, Equalizer, EqualizerConfig};
use luminous_lib::commands::equalizer::import_profile_into;
use luminous_lib::db::Database;
use luminous_lib::eq_store;
use std::path::PathBuf;

struct EqualizerTestEnv {
    equalizer: Equalizer,
    _dir: tempfile::TempDir,
    db: Database,
}

impl EqualizerTestEnv {
    fn new() -> Self {
        let mut eq = Equalizer::new();
        eq.update_format(44100, 2);
        let dir = tempfile::Builder::new()
            .prefix("luminous_eq_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        Self {
            equalizer: eq,
            _dir: dir,
            db,
        }
    }

    fn edit(&mut self, change: impl FnOnce(&mut EqualizerConfig)) {
        let mut config = self.equalizer.config();
        change(&mut config);
        self.equalizer.apply(&config);
    }

    fn user_presets(&self) -> Vec<eq_store::UserPreset> {
        let conn = self.db.pool.get().unwrap();
        eq_store::list(&conn).unwrap()
    }
}

#[test]
fn test_enable_and_disable_equalizer() {
    let mut env = EqualizerTestEnv::new();
    let samples = vec![0.5f32; 100];

    // Disabled initially
    env.equalizer.set_enabled(false);

    // Toggle "On" with preamp boost
    env.equalizer.set_enabled(true);
    env.edit(|config| config.preamp = 3.0);

    let mut processed = samples.clone();
    env.equalizer.process_interleaved(&mut processed);

    let modified = processed
        .iter()
        .zip(samples.iter())
        .any(|(&p, &o)| (p - o).abs() > 0.0001);
    assert!(
        modified,
        "Samples were not modified by equalizer process when enabled"
    );

    // Toggle "Off"
    env.equalizer.set_enabled(false);
    let mut bypassed = samples.clone();
    env.equalizer.process_interleaved(&mut bypassed);

    for (p, o) in bypassed.iter().zip(samples.iter()) {
        assert!(
            (p - o).abs() < 0.0001,
            "Samples were modified even though equalizer is disabled"
        );
    }
}

#[test]
fn test_adjust_band_gain_frequency_boost() {
    let mut env = EqualizerTestEnv::new();
    env.equalizer.set_enabled(true);

    // Set 1kHz band (index 5) to +6.0dB
    env.edit(|config| config.gains[5] = 6.0);

    let freq: f32 = 1000.0;
    let boost_db = 6.0f32;
    let fs = 44100.0;

    let mut input = Vec::new();
    for i in 0..882 {
        let sample_idx = i / 2;
        let t = sample_idx as f32 / fs;
        input.push((2.0 * std::f32::consts::PI * freq * t).sin() * 0.1);
    }

    let mut output = input.clone();
    env.equalizer.process_interleaved(&mut output);

    let in_peak = input.iter().map(|&x| x.abs()).fold(0.0f32, f32::max);
    let out_peak = output.iter().map(|&x| x.abs()).fold(0.0f32, f32::max);
    let gain = out_peak / in_peak;
    let gain_db = 20.0 * gain.log10();

    assert!(
        (gain_db - boost_db).abs() < 1.0,
        "Expected boost around {boost_db}dB, got {gain_db}dB"
    );
}

#[test]
fn test_load_builtin_preset() {
    let mut env = EqualizerTestEnv::new();
    env.equalizer.set_enabled(true);

    assert!(env.equalizer.load_builtin_preset("Rock"));

    let expected_gains = [4.0, 3.0, 1.0, -1.0, -2.0, -1.0, 1.0, 3.0, 3.5, 3.5];
    for (i, &expected) in expected_gains.iter().enumerate() {
        let actual = env.equalizer.config().gains[i];
        assert!(
            (actual - expected).abs() < 0.01,
            "Band {i} expected gain {expected}, got {actual}"
        );
    }

    // Process samples and verify filter coefficients recalculate
    let samples = vec![0.5f32; 100];
    let mut processed = samples.clone();
    env.equalizer.process_interleaved(&mut processed);

    let modified = processed
        .iter()
        .zip(samples.iter())
        .any(|(&p, &o)| (p - o).abs() > 0.0001);
    assert!(
        modified,
        "Filter coefficients were not recalculated or applied"
    );
}

#[test]
fn test_import_autoeq_profile() {
    let mut env = EqualizerTestEnv::new();
    env.equalizer.set_enabled(true);

    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/autoeq/Anker Soundcore Life Q20 ParametricEq.txt");
    let text = read_profile_file(&fixture_path).unwrap();

    let res = import_profile_into(
        &env.db,
        &mut env.equalizer,
        &text,
        "Anker Soundcore Life Q20",
    );
    let config = res.expect("import should succeed");

    let presets = env.user_presets();
    let preset = presets
        .iter()
        .find(|p| p.name == "Anker Soundcore Life Q20")
        .expect("preset saved");
    let key = format!("user:{}", preset.id);
    assert_eq!(config.active_preset.as_deref(), Some(key.as_str()));
    assert!(env.equalizer.config().enabled);

    let expected = parse_parametric_profile(&text).unwrap();
    assert_eq!(env.equalizer.config().parametric, expected.bands);
    assert_eq!(env.equalizer.config().preamp, -3.78);
}

#[test]
fn test_reject_profile_with_unsupported_filters() {
    let mut env = EqualizerTestEnv::new();
    env.equalizer.set_enabled(true);
    env.edit(|config| config.gains = [3.0; 10]);

    let before_config = env.equalizer.config();

    let text = "Preamp: -3 dB\nFilter 1: ON PK Fc 100 Hz Gain 2 dB Q 1\nFilter 2: ON LP Fc 80 Hz\n";
    let res = import_profile_into(&env.db, &mut env.equalizer, text, "Unsupported");

    let err = res.expect_err("import should fail for unsupported filter");
    let json: serde_json::Value = serde_json::from_str(&err).unwrap();
    assert_eq!(json["code"], "unsupported_filters");

    assert!(
        env.user_presets().is_empty(),
        "No user preset should be saved"
    );
    assert_eq!(
        env.equalizer.config(),
        before_config,
        "Equalizer settings should be unchanged"
    );
}
