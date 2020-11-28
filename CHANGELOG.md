Changelog for fishnet 2.x
=========================

New in v2.0.0
-------------

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
