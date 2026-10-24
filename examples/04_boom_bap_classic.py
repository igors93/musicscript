# 04 — Boom Bap 90s Classic (Golden Era Hip-Hop Groove)

bpm = 92       # [80, 108]
swing = 0.22   # [0.0, 0.45]
bass_drive = 0.35 # [0.0, 0.8]

beat = Beat("Boom Bap Classic", bpm=bpm, bars=2, swing=swing)

# Kick com ghost notes sincopadas antes do tempo forte
kick = beat.kick("X..o .... .x.X .... | X..o ...x ..X. ..o.", volume=0.96).drive(0.2)
snare = (
    beat.snare(".... X... .... X..o | .... X..o .... X...", volume=0.90)
    .reverb(room=0.35, mix=0.15)
)
hihat = beat.hihat("X.o. X.o. X.o. X.o.", volume=0.64).pan(0.15)
open_hat = beat.open_hat(".... ..x. .... ..x.", volume=0.48).pan(-0.25)

# Linha de baixo acústico/sub encorpado
bassline = (
    beat.bass808(
        "F1:3 . F1:2 Ab1:2 C2:4 . Eb2:2 E2:2 | F1:3 . F1:2 Ab1:2 C2:4 . Bb1:2 Ab1:2",
        volume=0.86,
    )
    .drive(bass_drive)
    .lowpass(cutoff=650)
)

# Stabs de cordas/brass sampleados nos contratempos
brass_stabs = (
    beat.pad(
        chords=["Fm7:3", ".:5", "Fm7:2", ".:6", "Dbmaj7:3", ".:5", "C7:4", ".:4"],
        octave=4,
        volume=0.68,
    )
    .adsr(attack=0.01, decay=0.25, sustain=0.4, release=0.15)
    .bandpass(cutoff=1400, resonance=0.35)
    .reverb(room=0.5, mix=0.25)
)

show_object(beat)
