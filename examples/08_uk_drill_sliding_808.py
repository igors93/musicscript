# 08 — UK Drill Sliding 808 (Counter-Snares & Octave Glides)

bpm = 144         # [136, 152]
glide_drive = 0.5 # [0.1, 0.9]

beat = Beat("UK Drill Sliding 808", bpm=bpm, bars=2)

# Bateria característica de UK Drill: Hi-Hat em padrão "3-3-2" (tresillo)
drill_hats = (
    beat.hihat(
        "X..x..x. X..x..r. | X..x..x. X..r..t.",
        name="Drill Counter-Hats",
        volume=0.76,
    )
    .pan(-0.15)
)

# Snare deslocado no quarto tempo (step 7 e step 13 do compasso)
drill_snare = (
    beat.snare("......X. ....X... | ......X. ....X..o", volume=0.90)
    .reverb(room=0.45, mix=0.18)
)

# Kick cirúrgico deixando espaço para os glides de 808
kick = beat.kick("X....... ..X..... | X.....X. ......X.", volume=0.96)

# 808 com glides longos de oitava ("C2->C3", "G2->Eb3")
drill_808 = (
    beat.bass808(
        "C2:6 . . . . C2->C3:2 Eb2->G2:2 C2:2 | C2:6 . . Ab1:4 G1->D3:2 G1->B2:2",
        name="Sliding 808",
        volume=0.94,
    )
    .drive(glide_drive)
)

# Strings / Pad sombrio em Dó menor
dark_strings = (
    beat.pad(
        chords=["Cm", "Ab", "Fm", "G7"],
        steps_per_chord=8,
        octave=4,
        volume=0.62,
    )
    .lowpass(cutoff=1600, resonance=0.30)
    .reverb(room=0.80, mix=0.35)
)

show_object(beat)
