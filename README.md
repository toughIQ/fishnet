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
* Fix some subtle edge cases wrt. threading in the current client
* Modernize by rewriting in Rust or dropping old Python versions

Roadmap
-------

* [x] Application structure
* [x] Signal handler
* [x] Logging
* [x] cpuid
* [x] Stockfish selection ~~and verification~~
* [x] systemd helper
* [x] Configuration
* [x] Incoming analysis
* [ ] Implement worker
  * [x] Standard analysis
  * [x] Failed work
  * [ ] Variant analysis
  * [ ] Use bundled Stockfish
  * [x] Protect engine from signals
  * [x] Backoff before restarting engine
* [ ] Performance based backoff
* [ ] Ouput for humans
  * [ ] Game links
  * [ ] TUI?
* [x] Outgoing analysis
* [ ] Backwards compatibility?
* [ ] Optimize submit/acquire?
* [ ] Move requests?
* [x] Shut down when outdated
* [ ] Fix Windows support
* [ ] Auto update
* [ ] ~~Warn about Python versions on old fishnet~~
* [ ] Test run
* [ ] Publish to main repository
