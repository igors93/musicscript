# 03 — Lo-Fi Chill Study (Swung Boom Drums, Rhodes Chords & Bitcrusher)

bpm = 82            # [65, 105]
swing = 0.24        # [0.0, 0.5]
lofi_cutoff = 1950  # [600, 5000]
crush_bits = 11     # [6, 16]

beat = Beat("Lo-Fi Chill Study", bpm=bpm, bars=2, swing=swing)

# Bateria abafada estilo sampler vintage SP-404
kick = (
    beat.kick("X... ..o. .x.. .... | X... ..o. .x.. ..o.", volume=0.88)
    .lowpass(cutoff=900)
)
snare = (
    beat.snare(".... X..o .... X... | .... X..o .... X..o", volume=0.78)
    .bitcrush(bits=crush_bits, downsample=2, mix=0.55)
    .lowpass(cutoff=2800)
)
hats = (
    beat.hihat("x.o. x.o. x.o. x.o.", volume=0.52)
    .bitcrush(bits=10, downsample=3, mix=0.5)
    .pan(-0.22)
)

# Progressão Jazz Lo-Fi em Piano Elétrico (Rhodes FM)
rhodes = (
    beat.keys(
        chords=["Cm9", "Fm7", "Bb7", "Ebmaj7"],
        steps_per_chord=8,
        octave=4,
        volume=0.74,
    )
    .chorus(depth=0.003, rate=1.4, mix=0.40)
    .lowpass(cutoff=lofi_cutoff, resonance=0.25)
    .reverb(room=0.65, damp=0.55, mix=0.28)
)

# Sub Bass macio acompanhando a fundamental dos acordes
sub_bass = beat.sub("C2:8 F1:8 Bb1:8 Eb2:8", volume=0.82)

# Melodia suave no topo
guitar_pluck = (
    beat.pluck(
        ". G4 Bb4 C5 . Bb4 G4 F4 | . Ab4 G4 Eb4 . F4 Eb4 D4",
        volume=0.58,
        pan=0.22,
    )
    .lowpass(cutoff=2600)
    .delay(time_steps=3, feedback=0.35, mix=0.25)
)

show_object(beat)
