# 05 — House Sidechain Groove (4x4 Club Kick & Pumping Supersaw Chords)

bpm = 126           # [115, 138]
pump_amount = 0.80  # [0.0, 1.0]
pad_cutoff = 3600   # [800, 9000]

beat = Beat("House Sidechain Groove", bpm=bpm, bars=2)

# Four-on-the-floor Club Kick
kick = beat.kick("X...X...X...X...", volume=0.98)
clap = beat.clap("....X.......X...", volume=0.84).reverb(room=0.5, mix=0.20)
open_hat = beat.open_hat("..X...X...X...X.", volume=0.65).pan(0.20)
shaker = beat.hihat("o.x.o.x.o.x.o.x.", volume=0.45).pan(-0.20)

# Offbeat House Bassline com ducking de sidechain
house_bass = (
    beat.synth(
        ". . A1 . . . A1 . . . F1 . . . G1 . | . . A1 . . . C2 . . . D2 . . . E2 .",
        name="Offbeat Bass",
        instrument="saw",
        volume=0.86,
    )
    .lowpass(cutoff=780, resonance=0.40)
    .sidechain(amount=pump_amount, period_steps=4)
)

# Supersaw Pad bombeando com o Kick (Sidechain a cada 4 steps)
chords = (
    beat.pad(
        chords=["Am7", "Fmaj7", "Dm7", "G7"],
        steps_per_chord=8,
        octave=4,
        volume=0.80,
    )
    .lowpass(cutoff=pad_cutoff, resonance=0.25)
    .chorus(depth=0.003, rate=1.8, mix=0.35)
    .sidechain(amount=pump_amount, period_steps=4)
    .reverb(room=0.70, mix=0.25)
)

# Pluck Arpeggio cristalino no topo
arp = (
    beat.arp(["A4", "C5", "E5", "G5", "A5"], rate=1.0, mode="updown", volume=0.58)
    .delay(time_steps=3, feedback=0.38, mix=0.30)
)

show_object(beat)
