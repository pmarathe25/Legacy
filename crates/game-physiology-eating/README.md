# game-physiology-eating

Physical food serving and consumption registered by `game-physiology` under
the physiology owner. Existing request and event keys and ownership are retained.

`ServeLocalMeals` settles independent local self-meals and voluntary care together.
Each offer runs `Serve` hooks; declined offers restore those hooks' effects. Stock
and permission operations retain exact ordered callbacks, while digestion, diet,
body and mood updates use their owners' batch APIs. `Fed` publication follows all
personal effects. The strict wave rejects overlapping/busy participants, lethal
effects and unsupported personal hooks; callers use an enclosing attempt before
ordered fallback. No scalar fallback occurs inside this request.
