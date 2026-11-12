//! Reads a TC folder: port of `Common::load` and the `archive_text` functions
//! (src/game/common.cpp, and common_model.hpp, since removed), giving the same data as
//! the C++ did for every TC it could load.
//!
//! As in the C++, a sound whose WAV header isn't 8-bit mono 22050 Hz is left
//! without samples, and a sprite file whose TGA header doesn't match the
//! expected size is left blank (and the palette unread). Missing files, missing
//! keys and files that end too early are errors, where the C++ threw.

use crate::exe::{CONSTANT_ORDER, HACKS, TEXTS};
use crate::model::*;
use crate::reader::Reader as Bin;
use crate::toml_reader::Reader;
use crate::Error;
use std::path::Path;

/// Sprite sets as the game allocates them: (width and height, count).
const SMALL_SPRITES: (usize, usize) = (7, 130);
const LARGE_SPRITES: (usize, usize) = (16, 110);
const TEXT_SPRITES: (usize, usize) = (4, 26);
const FONT_CHARS: usize = 250;

/// Reads the TC in `dir`, a folder written by tctool.
pub fn load_dir(dir: &Path) -> Result<Common, Error> {
    load(&mut |name| {
        let path = dir.join(name);
        std::fs::read(&path).map_err(|e| Error(format!("Could not read {}: {}", path.display(), e)))
    })
}

/// Reads a TC through `read`, which returns the contents of a file given its
/// path inside the TC folder, such as `weapons/bazooka.cfg`.
pub fn load(read: &mut dyn FnMut(&str) -> Result<Vec<u8>, Error>) -> Result<Common, Error> {
    let cfg = read("tc.cfg")?;
    let mut common = tc_cfg(&mut Reader::parse(&cfg, "tc.cfg")?)?;

    for sound in &mut common.sounds {
        let name = format!("sounds/{}.wav", sound.name);
        sound.data = wav(&read(&name)?)?;
    }

    let (size, count) = SMALL_SPRITES;
    common.small_sprites = SpriteSet {
        width: size,
        height: size,
        count,
        data: vec![0; size * size * count],
    };
    let file = read("sprites/small.tga")?;
    let mut palette = [[0u8; 3]; 256];
    tga(
        &file,
        "sprites/small.tga",
        size,
        size * count,
        &mut common.small_sprites.data,
        Some(&mut palette),
    )?;
    common.palette = palette;

    let (size, count) = LARGE_SPRITES;
    common.large_sprites = SpriteSet {
        width: size,
        height: size,
        count,
        data: vec![0; size * size * count],
    };
    let file = read("sprites/large.tga")?;
    tga(
        &file,
        "sprites/large.tga",
        size,
        size * count,
        &mut common.large_sprites.data,
        None,
    )?;

    let (size, count) = TEXT_SPRITES;
    common.text_sprites = SpriteSet {
        width: size,
        height: size,
        count,
        data: vec![0; size * size * count],
    };
    let file = read("sprites/text.tga")?;
    tga(
        &file,
        "sprites/text.tga",
        size,
        size * count,
        &mut common.text_sprites.data,
        None,
    )?;

    let file = read("sprites/font.tga")?;
    common.font = font(&file)?;

    let nobject_ids: Vec<String> = common.nobjects.iter().map(|n| n.id_str.clone()).collect();
    let sobject_ids: Vec<String> = common.sobjects.iter().map(|s| s.id_str.clone()).collect();

    for w in &mut common.weapons {
        let name = format!("weapons/{}.cfg", w.id_str);
        let id_str = std::mem::take(&mut w.id_str);
        *w = weapon(&mut Reader::parse(&read(&name)?, &name)?, &nobject_ids, &sobject_ids)?;
        w.id_str = id_str;
    }

    for n in &mut common.nobjects {
        let name = format!("nobjects/{}.cfg", n.id_str);
        let id_str = std::mem::take(&mut n.id_str);
        *n = nobject(&mut Reader::parse(&read(&name)?, &name)?, &nobject_ids, &sobject_ids)?;
        n.id_str = id_str;
    }

    for s in &mut common.sobjects {
        let name = format!("sobjects/{}.cfg", s.id_str);
        let id_str = std::mem::take(&mut s.id_str);
        *s = sobject(&mut Reader::parse(&read(&name)?, &name)?)?;
        s.id_str = id_str;
    }

    Ok(common)
}

fn id(r: &mut Reader) -> Result<String, Error> {
    String::from_utf8(r.str(None)?).map_err(|_| Error("tc.cfg: an object id isn't UTF-8".into()))
}

fn tc_cfg(r: &mut Reader) -> Result<Common, Error> {
    let (sounds, weapons, nobjects, sobjects) = r.obj(Some("types"), |r| {
        let sounds = r.arr("sounds", |r| {
            Ok(Sound {
                name: id(r)?,
                data: None,
            })
        })?;
        let weapons = r.arr("weapons", |r| {
            Ok(Weapon {
                id_str: id(r)?,
                ..Default::default()
            })
        })?;
        let nobjects = r.arr("nobjects", |r| {
            Ok(NObject {
                id_str: id(r)?,
                ..Default::default()
            })
        })?;
        let sobjects = r.arr("sobjects", |r| {
            Ok(SObject {
                id_str: id(r)?,
                ..Default::default()
            })
        })?;
        Ok((sounds, weapons, nobjects, sobjects))
    })?;
    let sobject_ids: Vec<String> = sobjects.iter().map(|s| s.id_str.clone()).collect();

    let mut bonus_rand_timer = [[0; 2]; 2];
    let mut bonus_frames = [0; 2];
    let mut bonus_sobjects = [0; 2];
    let mut ai_params = [[0; 7]; 2];

    let (textures, color_anim, materials, constants) = r.obj(Some("constants"), |r| {
        let bonuses: [(i32, i32, i32, i32); 2] = r.arr_n("bonuses", |r| {
            Ok((
                r.i32(Some("timer"))?,
                r.i32(Some("timerV"))?,
                r.i32(Some("frame"))?,
                r.reference("sobj", &sobject_ids)?,
            ))
        })?;
        for (i, &(timer, timer_v, frame, sobj)) in bonuses.iter().enumerate() {
            bonus_rand_timer[i] = [timer, timer_v];
            bonus_frames[i] = frame;
            bonus_sobjects[i] = sobj;
        }

        let textures: [Texture; 9] = r.arr_n("textures", |r| {
            Ok(Texture {
                m_frame: r.i32(Some("mframe"))?,
                r_frame: r.i32(Some("rframe"))?,
                s_frame: r.i32(Some("sframe"))?,
                n_draw_back: r.b("ndrawback")?,
            })
        })?;

        let color_anim: [(i32, i32); 4] = r.arr_n("colorAnim", |r| Ok((r.i32(Some("from"))?, r.i32(Some("to"))?)))?;

        let materials: [u8; 256] = r.arr_n("materials", |r| Ok(r.i32(None)? as u8))?;

        const KEYS: [&str; 7] = ["up", "down", "left", "right", "fire", "change", "jump"];
        r.obj(Some("aiparams"), |r| {
            for (i, key) in KEYS.into_iter().enumerate() {
                r.obj(Some(key), |r| {
                    ai_params[1][i] = r.i32(Some("on"))?;
                    ai_params[0][i] = r.i32(Some("off"))?;
                    Ok(())
                })?;
            }
            Ok(())
        })?;

        let mut constants = Vec::with_capacity(CONSTANT_ORDER.len());
        for &name in CONSTANT_ORDER {
            constants.push((name, r.i32(Some(name))?));
        }

        Ok((textures, color_anim, materials, constants))
    })?;

    let texts = r.obj(Some("texts"), |r| {
        TEXTS.iter().map(|&(name, _)| Ok((name, r.str(Some(name))?))).collect()
    })?;
    let hacks = r.obj(Some("hacks"), |r| {
        HACKS.iter().map(|&(name, _)| Ok((name, r.b(name)?))).collect()
    })?;

    Ok(Common {
        constants,
        texts,
        hacks,
        palette: [[0; 3]; 256],
        color_anim,
        materials,
        weapons,
        nobjects,
        sobjects,
        textures,
        bonus_rand_timer,
        bonus_sobjects,
        bonus_frames,
        ai_params,
        large_sprites: SpriteSet {
            width: 0,
            height: 0,
            count: 0,
            data: Vec::new(),
        },
        small_sprites: SpriteSet {
            width: 0,
            height: 0,
            count: 0,
            data: Vec::new(),
        },
        text_sprites: SpriteSet {
            width: 0,
            height: 0,
            count: 0,
            data: Vec::new(),
        },
        font: Vec::new(),
        sounds,
    })
}

fn weapon(r: &mut Reader, nobjects: &[String], sobjects: &[String]) -> Result<Weapon, Error> {
    r.obj(None, |r| {
        Ok(Weapon {
            name: r.str(Some("name"))?,
            id_str: String::new(),
            affect_by_worm: r.b("affectByWorm")?,
            shadow: r.b("shadow")?,
            laser_sight: r.b("laserSight")?,
            play_reload_sound: r.b("playReloadSound")?,
            worm_explode: r.b("wormExplode")?,
            expl_ground: r.b("explGround")?,
            worm_collide: r.b("wormCollide")?,
            collide_with_objects: r.b("collideWithObjects")?,
            affect_by_explosions: r.b("affectByExplosions")?,
            loop_anim: r.b("loopAnim")?,
            detect_distance: r.i32(Some("detectDistance"))?,
            blow_away: r.i32(Some("blowAway"))?,
            gravity: r.i32(Some("gravity"))?,
            launch_sound: r.i32(Some("launchSound"))?,
            loop_sound: r.i32(Some("loopSound"))?,
            explo_sound: r.i32(Some("exploSound"))?,
            speed: r.i32(Some("speed"))?,
            add_speed: r.i32(Some("addSpeed"))?,
            distribution: r.i32(Some("distribution"))?,
            parts: r.i32(Some("parts"))?,
            recoil: r.i32(Some("recoil"))?,
            mult_speed: r.i32(Some("multSpeed"))?,
            delay: r.i32(Some("delay"))?,
            loading_time: r.i32(Some("loadingTime"))?,
            ammo: r.i32(Some("ammo"))?,
            dirt_effect: r.i32(Some("dirtEffect"))?,
            leave_shells: r.i32(Some("leaveShells"))?,
            leave_shell_delay: r.i32(Some("leaveShellDelay"))?,
            fire_cone: r.i32(Some("fireCone"))?,
            bounce: r.i32(Some("bounce"))?,
            time_to_explo: r.i32(Some("timeToExplo"))?,
            time_to_explo_v: r.i32(Some("timeToExploV"))?,
            hit_damage: r.i32(Some("hitDamage"))?,
            blood_on_hit: r.i32(Some("bloodOnHit"))?,
            start_frame: r.i32(Some("startFrame"))?,
            num_frames: r.i32(Some("numFrames"))?,
            shot_type: r.i32(Some("shotType"))?,
            color_bullets: r.i32(Some("colorBullets"))?,
            splinter_amount: r.i32(Some("splinterAmount"))?,
            splinter_colour: r.i32(Some("splinterColour"))?,
            splinter_type: r.reference("splinterType", nobjects)?,
            splinter_scatter: r.i32(Some("splinterScatter"))?,
            obj_trail_type: r.reference("objTrailType", sobjects)?,
            obj_trail_delay: r.i32(Some("objTrailDelay"))?,
            part_trail_type: r.i32(Some("partTrailType"))?,
            part_trail_obj: r.reference("partTrailObj", nobjects)?,
            part_trail_delay: r.i32(Some("partTrailDelay"))?,
            create_on_exp: r.reference("createOnExp", sobjects)?,
            chain_explosion: r.b("chainExplosion")?,
        })
    })
}

fn nobject(r: &mut Reader, nobjects: &[String], sobjects: &[String]) -> Result<NObject, Error> {
    r.obj(None, |r| {
        Ok(NObject {
            id_str: String::new(),
            worm_explode: r.b("wormExplode")?,
            expl_ground: r.b("explGround")?,
            worm_destroy: r.b("wormDestroy")?,
            draw_on_map: r.b("drawOnMap")?,
            affect_by_explosions: r.b("affectByExplosions")?,
            blood_trail: r.b("bloodTrail")?,
            detect_distance: r.i32(Some("detectDistance"))?,
            gravity: r.i32(Some("gravity"))?,
            speed: r.i32(Some("speed"))?,
            speed_v: r.i32(Some("speedV"))?,
            distribution: r.i32(Some("distribution"))?,
            blow_away: r.i32(Some("blowAway"))?,
            bounce: r.i32(Some("bounce"))?,
            hit_damage: r.i32(Some("hitDamage"))?,
            blood_on_hit: r.i32(Some("bloodOnHit"))?,
            start_frame: r.i32(Some("startFrame"))?,
            num_frames: r.i32(Some("numFrames"))?,
            color_bullets: r.i32(Some("colorBullets"))?,
            create_on_exp: r.reference("createOnExp", sobjects)?,
            dirt_effect: r.i32(Some("dirtEffect"))?,
            splinter_amount: r.i32(Some("splinterAmount"))?,
            splinter_colour: r.i32(Some("splinterColour"))?,
            splinter_type: r.reference("splinterType", nobjects)?,
            blood_trail_delay: r.i32(Some("bloodTrailDelay"))?,
            leave_obj: r.reference("leaveObj", sobjects)?,
            leave_obj_delay: r.i32(Some("leaveObjDelay"))?,
            time_to_explo: r.i32(Some("timeToExplo"))?,
            time_to_explo_v: r.i32(Some("timeToExploV"))?,
        })
    })
}

fn sobject(r: &mut Reader) -> Result<SObject, Error> {
    r.obj(None, |r| {
        Ok(SObject {
            id_str: String::new(),
            shadow: r.b("shadow")?,
            start_sound: r.i32(Some("startSound"))?,
            num_sounds: r.i32(Some("numSounds"))?,
            anim_delay: r.i32(Some("animDelay"))?,
            start_frame: r.i32(Some("startFrame"))?,
            num_frames: r.i32(Some("numFrames"))?,
            detect_range: r.i32(Some("detectRange"))?,
            damage: r.i32(Some("damage"))?,
            blow_away: r.i32(Some("blowAway"))?,
            shake: r.i32(Some("shake"))?,
            flash: r.i32(Some("flash"))?,
            dirt_effect: r.i32(Some("dirtEffect"))?,
        })
    })
}

/// The samples of an 8-bit mono 22050 Hz WAV, made signed; `None` for any
/// other header. The header fields are read one at a time and the first one
/// that differs stops reading, as in the C++.
fn wav(file: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    let mut r = Bin::new(file, "a WAV file");
    if &r.bytes(4)? != b"RIFF" {
        return Ok(None);
    }
    r.u32()?; // The size, which is ignored.
    let expected: [(usize, u32); 10] = [
        (4, u32::from_le_bytes(*b"WAVE")),
        (4, u32::from_le_bytes(*b"fmt ")),
        (4, 16),
        (2, 1),
        (2, 1),
        (4, 22050),
        (4, 22050),
        (2, 1),
        (2, 8),
        (4, u32::from_le_bytes(*b"data")),
    ];
    for &(size, value) in &expected {
        let v = if size == 4 { r.u32()? } else { r.u16()? as u32 };
        if v != value {
            return Ok(None);
        }
    }
    let len = r.u32()? as usize;
    Ok(Some(r.bytes(len)?.iter().map(|&b| b.wrapping_sub(128)).collect()))
}

/// `readSpriteTga`: an uncompressed 8-bit palettized TGA of exactly
/// `width` x `height`, rows bottom to top, into `pixels`. With `palette`, the
/// TGA's palette is read too, as 6-bit VGA entries. A header that doesn't
/// match leaves everything untouched.
fn tga(
    file: &[u8],
    name: &'static str,
    width: usize,
    height: usize,
    pixels: &mut [u8],
    palette: Option<&mut [[u8; 3]; 256]>,
) -> Result<(), Error> {
    let mut r = Bin::new(file, name);
    let id_len = r.u8()? as usize;
    let header_ok = r.u8()? == 1
        && r.u8()? == 1
        && r.u16()? == 0
        && r.u16()? == 256
        && r.u8()? == 24
        && r.u16()? == 0
        && r.u16()? == 0;
    if !header_ok {
        return Ok(());
    }
    let image_width = r.u16()? as usize;
    let image_height = r.u16()? as usize;
    if !(r.u8()? == 8 && r.u8()? == 0) {
        return Ok(());
    }
    // The ID is skipped as far as the file goes.
    r.seek((r.pos() + id_len).min(file.len()))?;
    if image_width != width || image_height != height {
        return Ok(());
    }

    match palette {
        Some(palette) => {
            for entry in palette.iter_mut() {
                let b = r.u8()? >> 2;
                let g = r.u8()? >> 2;
                let rr = r.u8()? >> 2;
                *entry = [rr, g, b];
            }
        }
        None => r.seek((r.pos() + 256 * 3).min(file.len()))?,
    }

    for y in (0..height).rev() {
        pixels[y * width..(y + 1) * width].copy_from_slice(r.bytes(width)?);
    }
    Ok(())
}

/// The font strip: 50 marks a set pixel and 0 a clear one; the first other
/// value in a row ends that row and gives the character's width, so the last
/// row that ends early sets it.
fn font(file: &[u8]) -> Result<Vec<FontChar>, Error> {
    let mut pixels = vec![10u8; FONT_CHARS * 7 * 8];
    tga(file, "sprites/font.tga", 7, FONT_CHARS * 8, &mut pixels, None)?;

    Ok(pixels
        .chunks(56)
        .map(|src| {
            let mut ch = FontChar {
                data: [0; 56],
                width: 0,
            };
            for y in 0..8 {
                for x in 0..7 {
                    match src[y * 7 + x] {
                        0 => ch.data[y * 7 + x] = 0,
                        50 => ch.data[y * 7 + x] = 8,
                        _ => {
                            ch.width = x;
                            break;
                        }
                    }
                }
            }
            ch
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tc_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../TC/openliero")
    }

    /// Loads the bundled TC with `name` replaced by what `change` makes of it.
    fn load_with(name: &str, change: impl Fn(Vec<u8>) -> Vec<u8>) -> Result<Common, Error> {
        let dir = tc_dir();
        load(&mut |path| {
            let data = std::fs::read(dir.join(path)).map_err(|e| Error(e.to_string()))?;
            Ok(if path == name { change(data) } else { data })
        })
    }

    #[test]
    fn a_wav_in_another_format_has_no_samples() {
        // 16 bits per sample.
        let c = load_with("sounds/bump.wav", |mut d| {
            d[34] = 16;
            d
        })
        .unwrap();
        let bump = c.sounds.iter().find(|s| s.name == "bump").unwrap();
        assert!(bump.data.is_none());
        assert!(c.sounds.iter().filter(|s| s.name != "bump").all(|s| s.data.is_some()));
        assert!(load_with("sounds/bump.wav", |_| b"RIFX".to_vec())
            .unwrap()
            .sounds
            .iter()
            .any(|s| s.data.is_none()));
    }

    #[test]
    fn a_tga_of_another_size_is_left_blank() {
        let good = load_with("", |d| d).unwrap();
        // One pixel narrower: the sprites stay blank and the palette unread.
        let c = load_with("sprites/small.tga", |mut d| {
            d[12] -= 1;
            d
        })
        .unwrap();
        assert!(c.small_sprites.data.iter().all(|&p| p == 0));
        assert!(c.palette.iter().all(|e| *e == [0, 0, 0]));
        assert!(good.palette.iter().any(|e| *e != [0, 0, 0]));
        // A blank font strip is all 10s: every row ends at once.
        let c = load_with("sprites/font.tga", |mut d| {
            d[2] = 2;
            d
        })
        .unwrap();
        assert!(c.font.iter().all(|ch| ch.width == 0 && ch.data == [0; 56]));
    }

    #[test]
    fn files_that_end_early_are_errors() {
        assert!(load_with("sprites/large.tga", |d| d[..d.len() - 1].to_vec()).is_err());
        assert!(load_with("sounds/bump.wav", |d| d[..d.len() - 2].to_vec()).is_err());
        assert!(load_with("sounds/bump.wav", |d| d[..10].to_vec()).is_err());
    }

    #[test]
    fn bad_config_files_are_errors() {
        let replace = |from: &'static str, to: &'static str| {
            move |d: Vec<u8>| String::from_utf8(d).unwrap().replacen(from, to, 1).into_bytes()
        };
        assert!(
            load_with("weapons/bazooka.cfg", replace("bounce", "bounse")).is_err(),
            "a missing key"
        );
        assert!(
            load_with("weapons/bazooka.cfg", replace("= true", "= 1")).is_err(),
            "a wrong type"
        );
        assert!(
            load_with("weapons/bazooka.cfg", replace("\"large_explosion\"", "\"nothing\"")).is_err(),
            "an unknown id"
        );
        assert!(
            load_with("tc.cfg", replace("materials = [", "materials = [0, ")).is_err(),
            "257 materials"
        );
        assert!(
            load_with("weapons/bazooka.cfg", |mut d| {
                d.push(b'\n');
                d
            })
            .is_err(),
            "a line break at the end"
        );
        assert!(load_with("missing", |d| d).is_ok());
    }

    #[test]
    fn references_and_materials_read_back() {
        let c = load_with("", |d| d).unwrap();
        let bazooka = c.weapons.iter().find(|w| w.id_str == "bazooka").unwrap();
        assert_eq!(c.sobjects[bazooka.create_on_exp as usize].id_str, "large_explosion");
        assert_eq!(
            c.nobjects[bazooka.splinter_type as usize].id_str,
            "particle__small_damage"
        );
        assert!(c.weapons.iter().any(|w| w.obj_trail_type == -1), "null reads as -1");
        assert!(c.materials.iter().any(|&m| m != 0));
    }
}
