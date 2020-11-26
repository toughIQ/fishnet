Changelog for fishnet
=====================

Upcoming in v2.0.0
------------------

* Updated to Stockfish 12 NNUE.
* Fishnet is now distributed as a standalone binary instead of a Python module.
  To update, uninstall fishnet 1.x (`pip uninstall fishnet`) and install the
  new version. You can keep using your `fishnet.ini`.
* Removed `--threads-per-process`. Analysis is now always single-threaded for
  reproducibility, parallelizing over positions instead. This also allows
  finishing games more quickly, instead of starting to analyse multiple games
  at the same time.
* Removed `--memory`. All clients will now use the same setting for
  reproducibility.
* Removed `--stockfish-command` and `--engine-dir`.
  [Reproducible Stockfish builds](https://github.com/niklasf/fishnet-assets)
  for various CPU models now come bundled with the fishnet binary.
* Removed deprecated `--fixed-backoff`, `--no-fixed-backoff`,
  and `--setoption`.

v1.18.1
-------

* Lichess-hosted instances: Make `UCI_Elo` gradient steeper. Reintroduce depth
  limits to limit resource consumption of low levels.

v1.18.0
-------

* New command: Use `python -m fishnet systemd-user` to generate a systemd user
  service file.
* New command: Use `python -m fishnet benchmark` to try running the engine
  before getting a fishnet key.
* Fix process shutdown order with systemd.
* Fix race condition during shutdown on Python 2.7.
* Lichess-hosted instances: Use `UCI_Elo` instead of `Skill Level` and expand
  the range significantly. Low skill levels should now play much weaker.

v1.17.2
-------

* Reduce maximum move time from 20s to 6s. Clients that frequently hit this
  limit should be stopped in favor of clients with better hardware.
* Support future proof constants `--user-backlog short` and
  `--system-backlog long` (to be used instead of hardcoded durations).
* Fix some ignored command line flags during `python -m fishnet configure`
  and on intial run.

v1.17.1
-------

* Bring back `--threads-per-process`. Most contributors should not use this.

v1.17.0
-------

* Option to join only if a backlog is building up. Added `--user-backlog`
  and `--system-backlog` to configure threshold for oldest item in queue.
  Run `python -m fishnet configure` to rerun the setup dialog.
* Slow clients no longer work on young user requested analysis jobs. The
  threshold is continuously adjusted based on performance on other jobs.

v1.16.1
-------

* Fix false positive slowness warning.

v1.16.0
-------

* Removed `--threads-per-process`.
* Warning if client is unsustainably slow.
