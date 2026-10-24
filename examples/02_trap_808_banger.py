# 02 — Trap 808 Banger (Dark Phrygian Bells & Hi-Hat Rolls)
# Experimente alterar o BPM, o drive do 808 e o filtro do sino!

bpm = 142        # [120, 170]
drive_808 = 0.45 # [0.0, 0.9]
bell_mix = 0.32  # [0.0, 0.8]

beat = Beat("Trap 808 Banger", bpm=bpm, bars=2)

# Kick seco e pesado batendo junto com o 808
kick = beat.kick("X.....X...X..... | X.....X.....X...", volume=0.98)

# Clap no tempo 3 de cada compasso (half-time trap bounce) + snare de contratempo
clap = beat.clap("........X....... | ........X.......", volume=0.90).reverb(room=0.55, mix=0.22)
snare = beat.snare("............ .x.. | .....x.......x.o", volume=0.75)

# Hi-Hats com rolls rápidos ('r' = duplo 32nd, 't' = tercina rápida)
hihat = (
    beat.hihat("x.x.x.r.x.x.t.x. | x.x.r.x.x.t.x.r.", volume=0.68)
    .pan(-0.18)
)
open_hat = beat.open_hat("..x.......x..... | ..x.......x.....", volume=0.52).pan(0.25)

# 808 Bass com slides de pitch ("C2->G2") e saturação
bass_808 = (
    beat.bass808(
        "C2:4 . . C2:2 . . Eb2:3 . C2->G2:3 . | C2:4 . . C2:2 . . Ab1:3 . G1->D2:3 .",
        volume=0.92,
    )
    .drive(drive_808)
)

# Dark FM Trap Bell em escala menor harmônica / frígia
dark_bell = (
    beat.bell(
        "C5 . G4 Ab4 . G4 Eb4 . C5 . B4 Ab4 . G4 Eb4 .",
        volume=0.64,
        pan=0.15,
    )
    .delay(time_steps=3, feedback=0.40, mix=bell_mix)
    .reverb(room=0.75, mix=0.30)
)

show_object(beat)
