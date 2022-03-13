# Tantabus
Tantabus is a WIP hobby Chess and Chess960 engine.<br>
It is a "rewrite" of [Lunatic](https://github.com/analog-hors/lunatic).<br>
The code is restructured and a bit cleaner, and it also uses my own [`cozy-chess`](https://github.com/analog-hors/cozy-chess) library in place of [`chess`](https://github.com/jordanbray/chess).<br>
Play me on lichess: https://lichess.org/@/TantabusEngine.

## Features
### Movegen
- Fixed shift fancy black magic bitboards using [`cozy-chess`](https://github.com/analog-hors/cozy-chess)
### Search
- Principal variation search
- Aspiration windows
- Transposition table
    - "Always replace" replacement scheme
- Quiescence search
- Extensions
    - Check extensions
- Reductions
    - Late move reductions
- Pruning
    - Null move pruning
    - Futility pruning
    - Reverse futility pruning
    - Negative SEE moves pruned in QSearch
- Move ordering
    - Hash move
    - Capture moves
        - Losing captures delayed to last
    - Static exchange evaluation
    - Killer moves
    - History heuristic
### Evaluation
- [Automatically tuned with currently private tuner on the `lichess-big3-resolved` dataset](https://drive.google.com/file/d/1GfrNuDfD9Le-ZKKLxTHu0i3z6fbTn8JJ/view?usp=sharing)
- King relative symmetric piece-square tables
    - Dedicated passed pawn tables
- Mobility evaluation (simple pseudo-legal counting)
- Bishop pair bonus
- Rook on open file bonus
- Rook on semiopen file bonus
- Tapered/phased evaluation (using Fruit-like method)
### Time management
- Uses a fixed percentage of time left

## Thanks
Many engines have been very useful resources in the development of Tantabus.<br>
A (potentially incomplete) list of citations is listed in the code, annotated with `// CITE` comments.<br>
A (potentially incomplete) list of special thanks in no particular order:
- [Pali (Black Marlin author)](https://github.com/dsekercioglu/blackmarlin), for assisting me with various things during the development of Tantabus on top of being like, cool and stuff.
- [Jay (Berserk author)](https://github.com/jhonnold/berserk) for hosting the OpenBench instance that Tantabus develops on.
- [Andrew (OpenBench and Ethereal author)](https://github.com/AndyGrant/Ethereal) for creating OpenBench. It has been an immensely helpful tool for engine development.
- Other people I probably forgot about.
