# Changelog of `pidtree_mon`

## [0.2.1] - 2024-09-14

### 🚀 Features

- Extended output format specification
- Disabled all logging by default
- Updated to new with_daemon 0.2.0
- Handling worker failure by requesting daemon shutdown
- Factored float divisions out of measuring loads + docs

### 📚 Documentation

- Fixed incorrect description
- Fixed errors in example tmux configurations
- Updated readme

### ⚙️ Miscellaneous Tasks

- Updated upload-artifact from v2 to v4
- Added release --locked build to PR checks

## [0.2.0] - 2024-09-09

### 🚀 Features

- Switched to with_daemon
- A brand new load-calculation algorithm
- Simpler ticks retrieval

### 📚 Documentation

- Cosmetic changes in README.md

### ⚙️ Miscellaneous Tasks

- Added crates-publish action
- Release v0.2.0

## [0.1.1] - 2024-09-08

### Cargo.toml

- Fixed license declaration

## [0.1.0] - 2024-09-08

### PoC

- Client timeout + introduced clap for arg parsing

### Wip

- README + Cargo.toml metadata

<!-- generated by git-cliff -->
