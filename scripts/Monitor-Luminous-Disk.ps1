# Polls luminous.exe's own disk I/O every 2s, so you can tell whether the
# app itself is doing sustained reads/writes (vs. something else touching the
# same drive, e.g. indexing or cloud sync). Useful for tracking down
# "why is there disk activity after the scan should be done" reports — ties
# it to the app instead of guessing from the drive's activity light.
# Written while investigating https://github.com/esoltys/luminous/issues/166.
# Ctrl+C to stop.
while ($true) {
  $s = Get-Counter '\Process(luminous*)\IO Data Bytes/sec' -ErrorAction SilentlyContinue
  if ($s) {
    $total = ($s.CounterSamples | Measure-Object CookedValue -Sum).Sum
    "{0}  {1:N0} KB/s" -f (Get-Date -Format 'HH:mm:ss'), ($total/1KB)
  } else {
    "$(Get-Date -Format 'HH:mm:ss')  luminous.exe not running"
  }
  Start-Sleep -Seconds 2
}
