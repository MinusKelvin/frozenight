# Frozenight

NNUE chess engine written in Rust. [Play against it on lichess.org][lichess]

The current minimum supported Rust version is 1.57.0.

## Features

- [`cozy-chess`] for move generation
- Fail-soft negamax alpha beta search framework
- NNUE evaluation
  - 768 -> 16x2 -> 1
  - Training data generated through self-play, originally starting with a random network
- Quiescense search
  - MVV-LVA ordering
  - Check Evasions
  - Width limiting to prevent search explosion
- Late move reductions
- Null move pruning
- Transposition Table
  - Always replace
- Move ordering
  - Hash move
  - MVV-LVA captures
  - Killer heuristic (ordered near pawn captures pawn)
  - Relative history heuristic (almost)
  - Underpromotions last
- Time management
  - Uses at least 2.5% and no more than 10% of remaining time, prefering to stop soon

## Thanks

- Analog ([Tantabus]), for `cozy-chess` and helping me understand search techniques
- Pali ([Black Marlin]), for helping me understand NN training and search techniques
- Authors of the [chess programming wiki], for its wealth of knowledge

[lichess]: https://lichess.org/@/FrozenightEngine
[`cozy-chess`]: https://github.com/analog-hors/cozy-chess
[Tantabus]: https://github.com/analog-hors/tantabus
[Black Marlin]: https://github.com/dsekercioglu/blackmarlin
[chess programming wiki]: https://www.chessprogramming.org/Main_Page
