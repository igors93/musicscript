# 09 — Drift Phonk Cowbell (Distorted 808 & Iconic 808 Cowbell Melody)

bpm = 156           # [140, 175]
cowbell_drive = 0.45 # [0.0, 0.9]
bass_crush = 0.55   # [0.1, 0.9]

beat = Beat("Drift Phonk Cowbell", bpm=bpm, bars=2)

# Kick + 808 agressivo
kick = beat.kick("X... ..X. ..X. .... | X... ..X. ..X. ..X.", volume=0.98).drive(0.35)
snare = beat.snare(".... X... .... X... | .... X... .... X..x", volume=0.92)
hats = beat.hihat("x.x. x.r. x.x. x.t. | x.r. x.x. x.t. x.r.", volume=0.68)

# Melodia principal de 808 Cowbell saturada
cowbell_lead = (
    beat.cowbell(
        pattern=None,
        name="Phonk Cowbell",
        volume=0.88,
    )
    .notes("C4 . C4 Eb4 F4 . F#4 F4 | Eb4 . C4 Eb4 C4 . Bb3 B3")
    .drive(cowbell_drive)
    .reverb(room=0.45, mix=0.20)
)

# 808 Bass pesado acompanhando o riff
phonk_808 = (
    beat.bass808(
        "C2:4 . . C2:3 . F1:3 . G1:2 | C2:4 . . C2:3 . Ab1:3 . Bb1:2",
        name="Distorted 808",
        volume=0.95,
    )
    .drive(bass_crush)
)

show_object(beat)
