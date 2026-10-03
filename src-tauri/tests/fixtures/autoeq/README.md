# AutoEq fixtures

Real Equalizer APO parametric profiles, used by `src/eq_import.rs` tests to pin
the parser against files users actually download.

- `Sennheiser HD 600 ParametricEQ.txt` — from [AutoEq](https://github.com/jaakkopasanen/AutoEq)
  (`results/oratory1990/over-ear/Sennheiser HD 600`), © Jaakko Pasanen, MIT licence.
- `Anker Soundcore Life Q20 ParametricEq.txt` — exported from [autoeq.app](https://autoeq.app).

Keep them byte-for-byte as downloaded; the tests assert their exact values.
