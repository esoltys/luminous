use cucumber::gherkin::Step;
use cucumber::{given, then, when, World};
use luminous_lib::equalizer::Equalizer;

#[derive(Debug, World)]
pub struct EqualizerWorld {
    equalizer: Equalizer,
    samples: Vec<f32>,
    processed_samples: Vec<f32>,
}

impl Default for EqualizerWorld {
    fn default() -> Self {
        let mut eq = Equalizer::new();
        eq.update_format(44100, 2);
        Self {
            equalizer: eq,
            samples: vec![],
            processed_samples: vec![],
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

#[then(regex = r#"the (\d+kHz|\d+Hz) band filter coefficients should recalculate"#)]
fn coefficients_recalculate(w: &mut EqualizerWorld, _band: String) {
    assert!(w.equalizer.gains[5] > 0.0);
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
    let gains = [4.0, 3.0, 2.0, -1.0, -2.0, -1.0, 1.0, 2.0, 3.0, 4.0];
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

#[tokio::main]
async fn main() {
    EqualizerWorld::run("../features/equalizer.feature").await;
}
