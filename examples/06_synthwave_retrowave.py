# 06 — Synthwave 1984 (16th-Note Rolling Bass, Gated Snare & Neon Lead)

bpm = 110          # [95, 130]
bass_cutoff = 1350 # [400, 4000]
lead_delay = 0.35  # [0.0, 0.7]

beat = Beat("Synthwave 1984", bpm=bpm, bars=2)

# Bateria anos 80 (LinnDrum / Simmons style) com Snare bem reverberado
kick = beat.kick("X... .... X... .... | X... ..x. X... ....", volume=0.95)
snare = (
    beat.snare(".... X... .... X... | .... X... .... X..x", volume=0.90)
    .reverb(room=0.85, damp=0.2, mix=0.42)
)
hats = beat.hihat("x.x. x.x. x.x. x.x.", volume=0.55)
toms = beat.tom(".... .... .... .... | .... .... .... x.x.", notes="G3 F3 D3 C3", volume=0.80)

# Baixo Arpejado contínuo em semicolcheias (16th notes)
rolling_bass = (
    beat.synth(
        "C2 C2 C3 C2 C2 C2 C3 C2 Ab1 Ab1 Ab2 Ab1 Bb1 Bb1 Bb2 Bb1",
        name="Rolling Bass",
        instrument="saw",
        volume=0.88,
    )
    .lowpass(cutoff=bass_cutoff, resonance=0.42)
    .drive(0.25)
    .sidechain(amount=0.55, period_steps=4)
)

# Warm Analog Pad
retro_pad = (
    beat.pad(
        chords=["Cm", "Cm", "Abmaj7", "Bb"],
        steps_per_chord=8,
        octave=4,
        volume=0.68,
    )
    .chorus(depth=0.004, rate=1.2, mix=0.45)
    .sidechain(amount=0.60, period_steps=4)
)

# Neon Supersaw Lead
neon_lead = (
    beat.synth(
        "G4:3 Eb4:1 C5:4 Bb4:3 G4:1 F4:4 | G4:3 Eb4:1 D5:4 C5:6 . .",
        name="Neon Lead",
        instrument="supersaw",
        volume=0.74,
        pan=0.15,
    )
    .delay(time_steps=3, feedback=0.42, mix=lead_delay)
    .reverb(room=0.75, mix=0.30)
)

show_object(beat)
