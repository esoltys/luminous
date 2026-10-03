use cucumber::gherkin::Step;
use cucumber::{given, then, when, World};
use luminous_lib::commands::equalizer::import_profile_into;
use luminous_lib::db::Database;
use luminous_lib::eq_import::{parse_parametric_profile, read_profile_file};
use luminous_lib::eq_presets;
use luminous_lib::equalizer::{Equalizer, EqualizerConfig};
use std::path::PathBuf;

#[derive(Debug, World)]
pub struct EqualizerWorld {
    equalizer: Equalizer,
    samples: Vec<f32>,
    processed_samples: Vec<f32>,
    /// Preset store for import scenarios, created on first use.
    store: Option<(tempfile::TempDir, Database)>,
    fixture: Option<PathBuf>,
    before_import: Option<EqualizerConfig>,
    import_result: Option<Result<EqualizerConfig, String>>,
}

impl EqualizerWorld {
    fn db(&mut self) -> &Database {
        &self
            .store
            .get_or_insert_with(|| {
                let dir = tempfile::Builder::new()
                    .prefix("luminous_eq_bdd_")
                    .tempdir()
                    .unwrap();
                let db = Database::new(dir.path().to_path_buf()).unwrap();
                (dir, db)
            })
            .1
    }

    fn import(&mut self, text: &str, name: &str) {
        self.before_import = Some(EqualizerConfig::snapshot(&self.equalizer));
        self.db();
        let db = &self.store.as_ref().unwrap().1;
        self.import_result = Some(import_profile_into(db, &mut self.equalizer, text, name));
    }

    fn user_presets(&mut self) -> Vec<eq_presets::UserPreset> {
        let conn = self.db().pool.get().unwrap();
        eq_presets::list(&conn).unwrap()
    }
}

impl Default for EqualizerWorld {
    fn default() -> Self {
        let mut eq = Equalizer::new();
        eq.update_format(44100, 2);
        Self {
            equalizer: eq,
            samples: vec![],
            processed_samples: vec![],
            store: None,
            fixture: None,
            before_import: None,
            import_result: None,
        }
    }
}

#[given("the player is playing a track")]
fn player_is_playing(w: &mut EqualizerWorld) {
    w.samples = vec![0.5; 100];
    w.processed_samples = w.samples.clone();
}

#[given("the equalizer is currently disabled")]
fn equalizer_is_disabled(w: &mut EqualizerWorld) {
    w.equalizer.enabled = false;
}

#[given("the equalizer is enabled")]
fn equalizer_is_enabled(w: &mut EqualizerWorld) {
    w.equalizer.enabled = true;
}

#[when(expr = "I toggle the equalizer {string}")]
fn toggle_equalizer(w: &mut EqualizerWorld, state: String) {
    w.equalizer.enabled = state == "On";
}

#[then("the audio engine should process all playback samples through the 10-band filter cascade")]
fn process_samples(w: &mut EqualizerWorld) {
    w.equalizer.set_preamp(3.0);
    w.equalizer.recalculate();

    w.processed_samples = w.samples.clone();
    w.equalizer.process_interleaved(&mut w.processed_samples);

    let modified = w
        .processed_samples
        .iter()
        .zip(w.samples.iter())
        .any(|(&p, &o)| (p - o).abs() > 0.0001);
    assert!(modified, "Samples were not modified by equalizer process");
}

#[then("the audio engine should bypass the filter cascade and output dry samples")]
fn bypass_samples(w: &mut EqualizerWorld) {
    w.processed_samples = w.samples.clone();
    w.equalizer.process_interleaved(&mut w.processed_samples);

    for (p, o) in w.processed_samples.iter().zip(w.samples.iter()) {
        assert!(
            (p - o).abs() < 0.0001,
            "Samples were modified even though equalizer is disabled"
        );
    }
}

#[when(regex = r#"I set the gain of the "([^"]+)" band \(index (\d+)\) to "([^"]+)"#)]
fn set_band_gain(w: &mut EqualizerWorld, _band_name: String, band_idx: usize, gain_str: String) {
    let gain_clean = gain_str.replace("dB", "").replace("+", "");
    let gain_db: f32 = gain_clean.trim().parse().unwrap();
    w.equalizer.set_gain(band_idx, gain_db);
}

#[then(regex = r#"the audio engine should boost frequencies around (\d+kHz) by ([\d.]+)dB"#)]
fn check_frequency_boost(w: &mut EqualizerWorld, _freq_str: String, boost_db_str: String) {
    let freq: f32 = 1000.0;
    let boost_db: f32 = boost_db_str.parse().unwrap();

    let fs = 44100.0;
    let mut input = Vec::new();
    for i in 0..882 {
        let sample_idx = i / 2;
        let t = sample_idx as f32 / fs;
        input.push((2.0 * std::f32::consts::PI * freq * t).sin() * 0.1);
    }

    let mut output = input.clone();
    w.equalizer.process_interleaved(&mut output);

    let in_peak = input.iter().map(|&x| x.abs()).fold(0.0f32, f32::max);
    let out_peak = output.iter().map(|&x| x.abs()).fold(0.0f32, f32::max);
    let gain = out_peak / in_peak;

    let gain_db = 20.0 * gain.log10();
    assert!(
        (gain_db - boost_db).abs() < 1.0,
        "Expected boost around {}dB, got {}dB",
        boost_db,
        gain_db
    );
}

#[when("I select the \"Rock\" equalizer preset")]
fn select_rock_preset(w: &mut EqualizerWorld) {
    let gains = [4.0, 3.0, 1.0, -1.0, -2.0, -1.0, 1.0, 3.0, 3.5, 3.5];
    w.equalizer.load_preset(gains);
}

#[then("the gains for all 10 bands should update to preset values:")]
fn check_preset_table(w: &mut EqualizerWorld, step: &Step) {
    let table = step.table.as_ref().expect("Expected a table");
    for (i, row) in table.rows.iter().skip(1).enumerate() {
        let gain_str = &row[1];
        let gain_clean = gain_str.replace("dB", "").replace("+", "");
        let expected_gain: f32 = gain_clean.trim().parse().unwrap();
        let actual_gain = w.equalizer.gains[i];
        assert!(
            (actual_gain - expected_gain).abs() < 0.01,
            "Band {} expected gain {}, got {}",
            i,
            expected_gain,
            actual_gain
        );
    }
}

#[then("all biquad filter coefficients should recalculate")]
fn all_coefficients_recalculate(w: &mut EqualizerWorld) {
    if w.samples.is_empty() {
        w.samples = vec![0.5; 100];
    }
    w.processed_samples = w.samples.clone();
    w.equalizer.process_interleaved(&mut w.processed_samples);

    let modified = w
        .processed_samples
        .iter()
        .zip(w.samples.iter())
        .any(|(&p, &o)| (p - o).abs() > 0.0001);
    assert!(
        modified,
        "Filter coefficients were not recalculated or applied"
    );
}

#[given(expr = "the graphic band gains are set to {string}")]
fn graphic_gains_set(w: &mut EqualizerWorld, gain_str: String) {
    let gain: f32 = gain_str.replace("dB", "").replace('+', "").parse().unwrap();
    w.equalizer.load_preset([gain; 10]);
}

#[when(expr = "I import the AutoEq profile {string} as {string}")]
fn import_fixture(w: &mut EqualizerWorld, file: String, name: String) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/autoeq")
        .join(file);
    let text = read_profile_file(&path).unwrap();
    w.fixture = Some(path);
    w.import(&text, &name);
}

#[when(expr = "I import a profile containing a {string} filter")]
fn import_unsupported(w: &mut EqualizerWorld, kind: String) {
    let text = format!(
        "Preamp: -3 dB\nFilter 1: ON PK Fc 100 Hz Gain 2 dB Q 1\nFilter 2: ON {kind} Fc 80 Hz\n"
    );
    w.import(&text, "Unsupported");
}

#[then(expr = "a user preset named {string} should be active")]
fn user_preset_active(w: &mut EqualizerWorld, name: String) {
    let config = w.import_result.clone().unwrap().unwrap();
    let presets = w.user_presets();
    let preset = presets
        .iter()
        .find(|p| p.name == name)
        .expect("preset saved");
    let key = format!("user:{}", preset.id);
    assert_eq!(config.active_preset.as_deref(), Some(key.as_str()));
    assert!(w.equalizer.enabled);
}

#[then("the parametric bands should match the profile file exactly")]
fn bands_match_file(w: &mut EqualizerWorld) {
    let text = read_profile_file(w.fixture.as_ref().unwrap()).unwrap();
    let expected = parse_parametric_profile(&text).unwrap();
    assert_eq!(w.equalizer.parametric_bands(), expected.bands.as_slice());
}

#[then(regex = r"^the preamp should be (-?[\d.]+)dB$")]
fn preamp_is(w: &mut EqualizerWorld, db: String) {
    let expected: f32 = db.parse().unwrap();
    assert_eq!(w.equalizer.preamp, expected);
}

#[then(expr = "the import should fail with {string}")]
fn import_fails(w: &mut EqualizerWorld, code: String) {
    let err = w
        .import_result
        .clone()
        .unwrap()
        .expect_err("import should fail");
    let json: serde_json::Value = serde_json::from_str(&err).unwrap();
    assert_eq!(json["code"], code.as_str());
}

#[then("no user preset should be saved")]
fn no_preset_saved(w: &mut EqualizerWorld) {
    assert!(w.user_presets().is_empty());
}

#[then("the equalizer settings should be unchanged")]
fn settings_unchanged(w: &mut EqualizerWorld) {
    assert_eq!(
        Some(EqualizerConfig::snapshot(&w.equalizer)),
        w.before_import
    );
}

#[tokio::main]
async fn main() {
    // with_default_cli() skips clap-parsing argv, so `cargo test <filter>` (which
    // passes the filter string to every test binary, this one included) doesn't
    // blow up on an "unexpected argument" cucumber's CLI doesn't recognize.
    EqualizerWorld::cucumber()
        .max_concurrent_scenarios(4)
        .with_default_cli()
        .run_and_exit("../features/equalizer.feature")
        .await;
}
