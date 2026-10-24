# 07 — Acid Techno 303 (Warehouse 4x4 Kick & Resonant TB-303 Squelch)

bpm = 135          # [122, 150]
acid_cutoff = 2800 # [500, 8000]
acid_drive = 0.55  # [0.0, 0.95]

beat = Beat("Acid Techno 303", bpm=bpm, bars=2)

# Warehouse Rumble Kick
kick = beat.kick("X...X...X...X...", volume=0.98).drive(0.30)
clap = beat.clap("....X.......X...", volume=0.78).reverb(room=0.6, mix=0.25)
open_hat = beat.open_hat("..X...X...X...X.", volume=0.72).pan(0.18)
ride_hat = beat.hihat("x.x.x.x.x.x.x.r.", volume=0.56).pan(-0.18)

# Linha clássica de TB-303 Acid com slides ("->") e filtro ressonante
tb303 = (
    beat.acid(
        "A1 A2 A1->Bb1 G1 A1 E2 F2 A1 | A1 C3 A1 G1->A2 E1 F1 G1->A1 A2",
        name="TB-303 Acid",
        volume=0.88,
    )
    .lowpass(cutoff=acid_cutoff, resonance=0.78)
    .drive(acid_drive)
    .delay(time_steps=3, feedback=0.35, mix=0.26)
)

# Sub Rumble hipnótico nos contratempos
rumble = (
    beat.sub(". A1 A1 A1 . A1 A1 A1", volume=0.76)
    .lowpass(cutoff=160)
    .sidechain(amount=0.85, period_steps=4)
)

show_object(beat)
