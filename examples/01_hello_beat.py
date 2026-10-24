# 01 — Hello Beat (Starter Groove)
# Ajuste os sliders no painel "Parameters" para mudar o BPM e filtros em tempo real!

bpm = 120          # [80, 160]
swing = 0.08       # [0.0, 0.5]
lead_cutoff = 2400 # [400, 8000]

beat = Beat("Hello Beat", bpm=bpm, bars=2, swing=swing)

# Bateria (x = hit normal, X = acento, o = ghost note, r = roll 32nd, . = pausa)
kick = beat.kick("X...x...X...x..o", volume=0.95)
snare = beat.snare("....X.......X..o", volume=0.85).reverb(room=0.45, mix=0.18)
hats = beat.hihat("x.x.x.x.x.x.x.xr", volume=0.65).pan(-0.2)

# Baixo sintetizado
bass = (
    beat.sub("C2 . C2 Eb2 F2 . G2 Bb1", volume=0.88)
    .drive(0.25)
)

# Melodia Pluck com Delay sincronizado no tempo
melody = (
    beat.pluck("C4 Eb4 G4 Bb4 C5 Bb4 G4 Eb4", volume=0.72, pan=0.2)
    .lowpass(cutoff=lead_cutoff, resonance=0.35)
    .delay(time_steps=3, feedback=0.35, mix=0.28)
)

show_object(beat)
