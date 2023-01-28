# Frozenight

NNUE chess engine written in Rust. [Play against it on lichess.org][lichess]

The current minimum supported Rust version for the UCI binary is 1.59.0.

## Rating Lists

[CCRL 40/15][CCRL4040]:
- Frozenight 6.0: **3226**
- Frozenight 5.1: 3027
- Frozenight 5.0: 3000
- Frozenight 4.0: 2964
- Frozenight 3.0: 2842
- Frozenight 2.1: 2680
- Frozenight 2.0: 2614

[CCRL Blitz][CCRL404]:
- Frozenight 6.0: **3331**
- Frozenight 5.1: 3098
- Frozenight 5.0: 3085
- Frozenight 4.0: 3005
- Frozenight 3.0: 2891
- Frozenight 2.1: 2677
- Frozenight 1.0: 2448

[CCRL 40/2 FRC][CCRLFRC]:
- Frozenight 6.0: **3493**
- Frozenight 5.1: 3148
- Frozenight 5.0: 3103
- Frozenight 4.0: 2994
- Frozenight 3.0: 2761

## Features

- [`cozy-chess`] for move generation
- Principal Variation Search
- Aspiration windows
- LazySMP multithreading
- NNUE evaluation
  - 768 -> 384x2 (-> 1)x16
  - Network bucketing based on modified piece material values (Queen = 8)
    - This is based on game phase tuning in Koivisto done by Luecx
  - Training data generated through self-play, originally starting with a random network
  - Trained using (a modified version of) Pali's [`marlinflow`]
- Quiescense search
  - SEE ordering & pruning with MVV-LVA for ties
  - Check Evasions
- Check Extensions
- PV Extensions
- Late move reductions
- Late move pruning
- Null move pruning
- Reverse futility pruning, except using qsearch instead of static eval
- Internal Iterated Deepening
- Transposition Table
  - Depth-preferred with aging
- Move ordering
  - Hash move
  - SEE captures, losing captures last, with MVV-LVA for ties
  - Killer heuristic (ordered near pawn captures pawn)
  - Relative history heuristic (side-by-side piece-tosq and fromsq-tosq tables)
  - Underpromotions last
- Time management
  - Uses at least 2% remaining + increment/2
  - Avoids stopping in the middle of an iteration

## License

Frozenight is dual-licensed under the [MIT License](LICENSE-MIT) and [Apache License (Version 2.0)](LICENSE-APACHE) licesnses.

## Thanks

- Analog ([Tantabus]), for `cozy-chess` and helping me understand search techniques
- Pali ([Black Marlin]), for `marlinflow` and helping me understand NN training and search techniques
- Authors of the [chess programming wiki], for its wealth of knowledge

[lichess]: https://lichess.org/@/FrozenightEngine
[`cozy-chess`]: https://github.com/analog-hors/cozy-chess
[`marlinflow`]: https://github.com/dsekercioglu/marlinflow
[Tantabus]: https://github.com/analog-hors/tantabus
[Black Marlin]: https://github.com/dsekercioglu/blackmarlin
[chess programming wiki]: https://www.chessprogramming.org/Main_Page
[CCRL4040]: https://ccrl.chessdom.com/ccrl/4040/cgi/engine_details.cgi?eng=Frozenight%205.1.0%2064-bit
[CCRL404]: https://ccrl.chessdom.com/ccrl/404/cgi/engine_details.cgi?eng=Frozenight%205.0.0%2064-bit
[CCRLFRC]: https://ccrl.chessdom.com/ccrl/404FRC/cgi/engine_details.cgi?eng=Frozenight%205.1.0
