//! Loading levels and making them from the settings.

use super::*;
use crate::material::{MATERIAL_BACKGROUND, MATERIAL_DIRT};
use crate::settings::new_settings;

/// A TC's level tables: colour 0 is background, 5 dirt, the rest nothing.
struct Tables {
    materials: [Material; 256],
    textures: [Texture; TEXTURE_COUNT],
    sprites: Vec<u8>,
}

impl Tables {
    fn new() -> Self {
        let mut materials = [Material::default(); 256];
        materials[5].flags = MATERIAL_DIRT;
        materials[0].flags = MATERIAL_BACKGROUND;
        let mut textures = [Texture::default(); TEXTURE_COUNT];
        for t in textures.iter_mut() {
            t.r_frame = 1;
        }
        Tables {
            materials,
            textures,
            sprites: vec![0; 110 * SPRITE_PIXELS],
        }
    }

    fn common(&self) -> Common<'_> {
        Common {
            materials: &self.materials,
            textures: &self.textures,
            large_sprites: &self.sprites,
        }
    }
}

fn exepal() -> [Color; 256] {
    std::array::from_fn(|i| Color {
        r: i as u8 % 64,
        g: 1,
        b: 2,
        unused: 0,
    })
}

const PIXELS: usize = (LEVEL_WIDTH * LEVEL_HEIGHT) as usize;

/// A .LEV file of colour 5, then `rest`.
fn lev(rest: &[u8]) -> Vec<u8> {
    let mut b = vec![5; PIXELS];
    b.extend_from_slice(rest);
    b
}

fn powerlevel(palette_len: usize) -> Vec<u8> {
    let mut rest = b"POWERLEVEL".to_vec();
    rest.extend((0..palette_len).map(|i| (i % 256) as u8 | 0xc0));
    lev(&rest)
}

#[test]
fn a_level_file_has_its_pixels_and_materials() {
    let t = Tables::new();
    let mut l = LevelBox::new();
    l.data_mut().origpal[3].r = 9;
    assert!(l.load(&t.common(), &exepal(), true, &lev(&[])));
    assert_eq!((l.data().width, l.data().height), (LEVEL_WIDTH, LEVEL_HEIGHT));
    assert!(l.pixels().iter().all(|&p| p == 5));
    assert!(l.materials.iter().all(|m| m.flags == MATERIAL_DIRT));
    assert_eq!(l.data().origpal, exepal());

    // Cut short: what there is is read, and loading fails.
    let mut l = LevelBox::new();
    let mut short = lev(&[]);
    short.truncate(PIXELS - 1);
    assert!(!l.load(&t.common(), &exepal(), true, &short));
    assert_eq!((l.pixels()[0], l.pixels()[PIXELS - 1]), (5, 0));
}

#[test]
fn a_powerlevel_palette_is_read_when_asked_for() {
    let t = Tables::new();
    let pal = exepal();
    let mut l = LevelBox::new();
    assert!(l.load(&t.common(), &pal, true, &powerlevel(768)));
    let o = l.data().origpal;
    // The top two bits of each component are dropped.
    assert_eq!((o[0].r, o[0].g, o[0].b), (0, 1, 2));
    assert_eq!(
        (o[255].r, o[255].g, o[255].b),
        ((765 % 256) as u8 & 63, (766 % 256) as u8 & 63, (767 % 256) as u8 & 63)
    );

    // Not asked for, or another mark: the TC's.
    assert!(l.load(&t.common(), &pal, false, &powerlevel(768)));
    assert_eq!(l.data().origpal, pal);
    assert!(l.load(&t.common(), &pal, true, &lev(b"POWER")));
    assert_eq!(l.data().origpal, pal);
    // The mark with a palette cut short fails.
    assert!(!l.load(&t.common(), &pal, true, &powerlevel(767)));
}

#[test]
fn the_settings_level_file_gets_lev_added_and_falls_back_to_random() {
    let dir = "target/level-box-tests";
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(format!("{dir}/MAP.LEV"), lev(&[])).unwrap();
    let t = Tables::new();
    {
        let shared = new_settings();
        let mut s = shared.borrow_mut();
        s.data.random_level = false;
        s.data.shadow = false;
        s.data.level_file = format!("{dir}/MAP").into_bytes();
        let mut l = LevelBox::new();
        let mut rand = Rand::new(3);
        l.generate_from_settings(&t.common(), &exepal(), &s, &mut rand);
        assert!(l.pixels().iter().all(|&p| p == 5));
        assert_eq!(
            (l.data().old_random_level, l.old_level_file()),
            (false, s.data.level_file.as_slice())
        );
        assert_eq!(rand, Rand::new(3));

        // A file that isn't there makes a random level.
        s.data.level_file = format!("{dir}/NONE").into_bytes();
        l.generate_from_settings(&t.common(), &exepal(), &s, &mut rand);
        assert_ne!(rand, Rand::new(3));
        assert!(l.pixels().iter().any(|&p| p != 5));

        // Shadows go on a loaded level too: the bottom row's background
        // becomes floor.
        std::fs::write(format!("{dir}/BG.LEV"), vec![0; PIXELS]).unwrap();
        s.data.level_file = format!("{dir}/BG.LEV").into_bytes();
        l.generate_from_settings(&t.common(), &exepal(), &s, &mut rand);
        assert_eq!(l.pixels()[PIXELS - 1], 0);
        s.data.shadow = true;
        l.generate_from_settings(&t.common(), &exepal(), &s, &mut rand);
        assert_eq!((l.pixels()[PIXELS - 1], l.pixels()[0]), (13, 0));
    }
}
