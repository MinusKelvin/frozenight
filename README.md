# Frozenight

NNUE chess engine written in Rust. [Play against it on lichess.org][lichess]

The current minimum supported Rust version for the UCI binary is 1.57.0.

## Features

- [`cozy-chess`] for move generation
- Principal Variation Search
- NNUE evaluation
  - 768 -> 16x2 -> 1
  - Training data generated through self-play, originally starting with a random network
- Quiescense search
  - MVV-LVA ordering
  - Check Evasions
  - Late move pruning to prevent search explosion
- Late move reductions
- Null move pruning
- Reverse futility pruning, except using qsearch instead of static eval
- Transposition Table
  - Depth-preferred with aging
- Move ordering
  - Hash move
  - MVV-LVA captures
  - Killer heuristic (ordered near pawn captures pawn)
  - Relative history heuristic (side-by-side piece-tosq and fromsq-tosq tables)
  - Underpromotions last
- Time management
  - Uses at least 2% remaining + increment/2
  - Avoids stopping in the middle of an iteration

## Thanks

- Analog ([Tantabus]), for `cozy-chess` and helping me understand search techniques
- Pali ([Black Marlin]), for helping me understand NN training and search techniques
- Authors of the [chess programming wiki], for its wealth of knowledge

[lichess]: https://lichess.org/@/FrozenightEngine
[`cozy-chess`]: https://github.com/analog-hors/cozy-chess
[Tantabus]: https://github.com/analog-hors/tantabus
[Black Marlin]: https://github.com/dsekercioglu/blackmarlin
[chess programming wiki]: https://www.chessprogramming.org/Main_Page
