fishnet: distributed Stockfish analysis for lichess.org
=======================================================

Experimental rewrite
--------------------

This is an experimental rewrite of [fishnet](https://github.com/niklasf/fishnet).
Look there for the current version.

Goals
-----

* Update to Stockfish 12 NNUE
* Reproducible analysis
* Modernize by rewriting in Rust or dropping old Python versions

Roadmap
-------

* [*] Application structure
* [*] Signal handler
* [*] Logging
* [*] cpuid
* [*] Stockfish selection ~~and verification~~
* [*] systemd helper
* [*] Configuration
* [ ] Incoming analysis
* [ ] Implement worker
* [ ] Outgoing analysis
* [ ] Backwards compatibility?
* [ ] Optimize submit/acquire?
* [ ] Move requests?
* [ ] Shut down when outdated
* [ ] Auto update
* [ ] ~~Warn about Python versions on old fishnet~~
* [ ] Test run
* [ ] Publish to main repository
