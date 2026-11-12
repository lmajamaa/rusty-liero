//! Reads game data out of LIERO.EXE, LIERO.CHR and LIERO.SND.
//! Port of `src/tc_tool/common_exereader.cpp`; offsets are into Liero 1.33's files.

use crate::model::*;
use crate::rand::Mwc;
use crate::reader::Reader;
use crate::Error;
use std::collections::HashMap;

type R<'a, 'b> = &'a mut Reader<'b>;

// Constants as (name, offset of low word, offset of high part).
const SINT32: &[(&str, usize, usize)] = &[("NRInitialLength", 0x32D7, 0x32DD), ("NRAttachLength", 0xA679, 0xA67F)];

const SINT24: &[(&str, usize, usize)] = &[
    ("MinBounceUp", 0x3B7D, 0x3B74),
    ("MinBounceDown", 0x3B00, 0x3AF7),
    ("MinBounceLeft", 0x3A83, 0x3A7A),
    ("MinBounceRight", 0x3A06, 0x39FD),
    ("WormGravity", 0x3BDE, 0x3BD7),
    ("WalkVelLeft", 0x3F97, 0x3F9D),
    ("MaxVelLeft", 0x3F8C, 0x3F83),
    ("WalkVelRight", 0x4018, 0x401E),
    ("MaxVelRight", 0x400D, 0x4004),
    ("JumpForce", 0x3327, 0x332D),
    ("MaxAimVelLeft", 0x30F2, 0x30E9),
    ("AimAccLeft", 0x30FD, 0x3103),
    ("MaxAimVelRight", 0x311A, 0x3111),
    ("AimAccRight", 0x3125, 0x312B),
    ("NinjaropeGravity", 0xA895, 0xA89B),
    ("NRMinLength", 0x3206, 0x31FD),
    ("NRMaxLength", 0x3229, 0x3220),
    ("BonusGravity", 0x72C3, 0x72C9),
    ("BObjGravity", 0x744A, 0x7450),
    // WormFloat hack
    ("WormFloatPower", 0x29DB, 0x29E1),
];

const UINT16: &[(&str, usize)] = &[("BloodLimit", 0xE686)];

const SINT16: &[(&str, usize)] = &[
    ("WormFricMult", 0x39BD),
    ("WormFricDiv", 0x39C7),
    ("WormMinSpawnDistLast", 0x242E),
    ("WormMinSpawnDistEnemy", 0x244B),
    ("WormSpawnRectX", 0x4913),
    ("WormSpawnRectY", 0x4925),
    ("WormSpawnRectW", 0x490B),
    ("WormSpawnRectH", 0x491D),
    ("AimFricMult", 0x3003),
    ("AimFricDiv", 0x300D),
    ("NRThrowVelX", 0x329B),
    ("NRThrowVelY", 0x32BF),
    ("NRForceShlX", 0xA8AD),
    ("NRForceDivX", 0xA8B7),
    ("NRForceShlY", 0xA8DA),
    ("NRForceDivY", 0xA8E4),
    ("NRForceLenShl", 0xA91E),
    ("BonusBounceMul", 0x731F),
    ("BonusBounceDiv", 0x7329),
    ("BonusFlickerTime", 0x87B8),
    ("BonusDropChance", 0xBECA),
    ("SplinterLarpaVelDiv", 0x677D),
    ("SplinterCracklerVelDiv", 0x67D0),
    // WormFloat hack
    ("WormFloatLevel", 0x29D3),
    // BonusSpawnRect hack. W and H are used even when the hack isn't enabled.
    ("BonusSpawnRectX", 0x2319),
    ("BonusSpawnRectY", 0x2327),
    ("BonusSpawnRectW", 0x2311),
    ("BonusSpawnRectH", 0x231F),
];

const UINT8: &[(&str, usize)] = &[
    ("AimMaxRight", 0x3030),
    ("AimMinRight", 0x304A),
    ("AimMaxLeft", 0x3066),
    ("AimMinLeft", 0x3080),
    ("NRColourBegin", 0x10FD2),
    ("NRColourEnd", 0x11069),
    ("BonusExplodeRisk", 0x2DB2),
    ("BonusHealthVar", 0x2D56),
    ("BonusMinHealth", 0x2D5D),
    ("LaserWeapon", 0x7255),
    ("FirstBloodColour", 0x2388),
    ("NumBloodColours", 0x2381),
    ("RemExpObject", 0x8F8B),
];

const SINT8: &[(&str, usize)] = &[
    ("NRPullVel", 0x31D0),
    ("NRReleaseVel", 0x31F0),
    // FallDamage hack
    ("FallDamageRight", 0x3A0E),
    ("FallDamageLeft", 0x3A8B),
    ("FallDamageDown", 0x3B08),
    ("FallDamageUp", 0x3B85),
    ("BloodStepUp", 0xE67B),
    ("BloodStepDown", 0xE68E),
];

/// Constants in the order they are written to tc.cfg (LIERO_CDEFS).
pub const CONSTANT_ORDER: &[&str] = &[
    "NRInitialLength",
    "NRAttachLength",
    "MinBounceUp",
    "MinBounceDown",
    "MinBounceLeft",
    "MinBounceRight",
    "WormGravity",
    "WalkVelLeft",
    "MaxVelLeft",
    "WalkVelRight",
    "MaxVelRight",
    "JumpForce",
    "MaxAimVelLeft",
    "AimAccLeft",
    "MaxAimVelRight",
    "AimAccRight",
    "NinjaropeGravity",
    "NRMinLength",
    "NRMaxLength",
    "BonusGravity",
    "WormFricMult",
    "WormFricDiv",
    "WormMinSpawnDistLast",
    "WormMinSpawnDistEnemy",
    "WormSpawnRectX",
    "WormSpawnRectY",
    "WormSpawnRectW",
    "WormSpawnRectH",
    "AimFricMult",
    "AimFricDiv",
    "NRThrowVelX",
    "NRThrowVelY",
    "NRForceShlX",
    "NRForceDivX",
    "NRForceShlY",
    "NRForceDivY",
    "NRForceLenShl",
    "BonusBounceMul",
    "BonusBounceDiv",
    "BonusFlickerTime",
    "AimMaxRight",
    "AimMinRight",
    "AimMaxLeft",
    "AimMinLeft",
    "NRPullVel",
    "NRReleaseVel",
    "NRColourBegin",
    "NRColourEnd",
    "BonusExplodeRisk",
    "BonusHealthVar",
    "BonusMinHealth",
    "LaserWeapon",
    "FirstBloodColour",
    "NumBloodColours",
    "BObjGravity",
    "BonusDropChance",
    "SplinterLarpaVelDiv",
    "SplinterCracklerVelDiv",
    "BloodStepUp",
    "BloodStepDown",
    "BloodLimit",
    "FallDamageRight",
    "FallDamageLeft",
    "FallDamageDown",
    "FallDamageUp",
    "WormFloatLevel",
    "WormFloatPower",
    "BonusSpawnRectX",
    "BonusSpawnRectY",
    "BonusSpawnRectW",
    "BonusSpawnRectH",
    "RemExpObject",
];

/// Pascal strings, in the order they are written to tc.cfg (LIERO_SDEFS).
pub const TEXTS: &[(&str, usize)] = &[
    ("InitSound", 0x177F),
    ("LoadingSounds", 0x18F2),
    ("LoadingAndThinking", 0xFB92),
    ("OK", 0xFBA8),
    ("OK2", 0x190E),
    ("PressAnyKey", 0xFBAB),
    ("CommittedSuicideMsg", 0xE70C),
    ("KilledMsg", 0xE71F),
    ("YoureIt", 0x75C5),
    ("Init_BaseIO", 0x17DD),
    ("Init_IRQ", 0x17E5),
    ("Init_DMA8", 0x17EE),
    ("Init_DMA16", 0x17F8),
    ("Init_DSPVersion", 0x181E),
    ("Init_Colon", 0x182B),
    ("Init_16bit", 0x182F),
    ("Init_Autoinit", 0x1840),
    ("Init_XMSSucc", 0x189D),
    ("Init_FreeXMS", 0x18C5),
    ("Init_k", 0x18D8),
    ("Random", 0xD6E3),
    ("Random2", 0xD413),
    ("RegenLevel", 0xD41A),
    ("ReloadLevel", 0xD42D),
    ("Copyright", 0xFB60),
    ("Copyright2", 0xE693),
    ("SelWeap", 0xA9C0),
    ("LevelRandom", 0xA9D5),
    ("LevelIs1", 0xA9E3),
    ("LevelIs2", 0xA9EC),
    ("Randomize", 0xA9F4),
    ("Done", 0xA9EE),
    ("Reloading", 0x7583),
    ("PressFire", 0x7590),
    ("Kills", 0x75A4),
    ("Lives", 0x75AC),
    ("SelLevel", 0xD6F2),
    ("Weapon", 0xD700),
    ("Availability", 0xD707),
    ("NoWeaps", 0xD714),
];

/// A hack is active when all of its (offset, byte) indicators match.
/// Listed in the order they are written to tc.cfg (LIERO_HDEFS).
pub const HACKS: &[(&str, &[(usize, u8)])] = &[
    (
        "FallDamage",
        &[(0x3A0A, 0x26), (0x3A87, 0x26), (0x3B04, 0x26), (0x3B81, 0x26)],
    ),
    // One byte only, because ProMode has a silly jump destination
    ("BonusReloadOnly", &[(0x2DB1, 0xEB)]),
    // First bytes of the add instructions that offset the spawn
    ("BonusSpawnRect", &[(0x2318, 0x05), (0x2323, 0x05)]),
    ("BonusOnlyHealth", &[(0x228B, 0xB0), (0x228C, 0x02)]),
    ("BonusOnlyWeapon", &[(0x228B, 0xB0), (0x228C, 0x01)]),
    ("BonusDisable", &[(0xBED3, 0xEB)]),
    // 0x26 starts the sub instruction, 0x34 is the offset to part of the worm's velY
    ("WormFloat", &[(0x29D7, 0x26), (0x29DA, 0x34)]),
    // Start of the mov instruction that zeroes the timeout counter
    ("RemExp", &[(0x8fc9, 0x26), (0x8fca, 0xc7)]),
    ("SignedRecoil", &[(0x38AC, 0x98), (0x38EC, 0x98)]),
    ("AirJump", &[(0x3313, 0xEB), (0x3314, 0x06)]),
    ("MultiJump", &[(0x331B, 0xEB), (0x331C, 0x06)]),
];

const SOBJECT_NAMES: [&str; 14] = [
    "Large explosion",
    "Medium explosion",
    "Small explosion",
    "Hellraider smoke",
    "Zimm flash",
    "Nuke smoke",
    "Flashing pixel",
    "Teleport flash",
    "Small explosion, silent",
    "Very small explosion, silent",
    "Medium explosion, smaller",
    "Large explosion, smaller",
    "Medium explosion, bigger",
    "Unknown",
];

const NOBJECT_NAMES: [&str; 24] = [
    "Worm 1 parts",
    "Worm 2 parts",
    "Particle, disappearing",
    "Particle, small damage",
    "Particle, medium damage",
    "Particle, larger damage",
    "Blood",
    "Shells",
    "Clusterbomb bombs",
    "Large nukes",
    "Hellraider bullets",
    "Small nukes",
    "Napalm fireballs",
    "Dirt",
    "Chiquitabomb bombs",
    "Grasshopper 1",
    "Grasshopper 2",
    "Grasshopper 3",
    "Grasshopper 4",
    "Grasshopper 5",
    "Flag 1",
    "Flag 2",
    "Grasshopper 6",
    "Grasshopper 7",
];

// Field readers, matching Read8 / Read16 / Read32 / ReadBool / Dec<Read8> in the C++ code.
fn r8(r: R) -> Result<i32, Error> {
    Ok(r.u8()? as i32)
}
fn r16(r: R) -> Result<i32, Error> {
    Ok(r.u16()? as i16 as i32)
}
fn r32(r: R) -> Result<i32, Error> {
    Ok(r.u32()? as i32)
}
fn rbool(r: R) -> Result<bool, Error> {
    Ok(r.u8()? != 0)
}
fn rbool_i32(r: R) -> Result<i32, Error> {
    Ok((r.u8()? != 0) as i32)
}
fn rdec8(r: R) -> Result<i32, Error> {
    Ok(r.u8()? as i32 - 1)
}

/// The EXE stores each field for all items in a row, so read one field across every item.
macro_rules! column {
    ($r:expr, $items:expr, $read:ident, $($field:ident).+) => {
        for item in $items.iter_mut() {
            item.$($field).+ = $read($r)?;
        }
    };
}

pub fn load(exe: &[u8], chr: &[u8], snd: &[u8]) -> Result<Common, Error> {
    let mut exe = Reader::new(exe, "LIERO.EXE");
    let mut chr = Reader::new(chr, "LIERO.CHR");
    let mut snd = Reader::new(snd, "LIERO.SND");

    let (constants, texts, hacks) = load_constants(&mut exe)?;
    let font = load_font(&mut exe)?;
    let (palette, color_anim) = load_palette(&mut exe)?;
    let materials = load_materials(&mut exe)?;
    let (weapons, sobjects, nobjects) = load_objects(&mut exe)?;
    let textures = load_textures(&mut exe)?;

    // loadOthers
    let mut bonus_rand_timer = [[0; 2]; 2];
    exe.seek(0x1C1E2)?;
    for i in 0..2 {
        for timer in &mut bonus_rand_timer {
            timer[i] = exe.u16()? as i32;
        }
    }
    let mut ai_params = [[0; 7]; 2];
    exe.seek(0x1AEEE + 2)?;
    for row in ai_params.iter_mut() {
        for k in row.iter_mut() {
            *k = exe.u16()? as i32;
        }
    }
    exe.seek(0x1C1E0)?;
    let bonus_sobjects = [rdec8(&mut exe)?, rdec8(&mut exe)?];

    // loadGfx
    exe.seek(0x1C1DE)?;
    let bonus_frames = [r8(&mut exe)?, r8(&mut exe)?];
    let (large_sprites, small_sprites, text_sprites) = load_sprites(&mut chr)?;

    let sounds = load_sounds(&mut snd)?;

    Ok(Common {
        constants,
        texts,
        hacks,
        palette,
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
        large_sprites,
        small_sprites,
        text_sprites,
        font,
        sounds,
    })
}

#[allow(clippy::type_complexity)]
fn load_constants(
    exe: R,
) -> Result<
    (
        Vec<(&'static str, i32)>,
        Vec<(&'static str, Vec<u8>)>,
        Vec<(&'static str, bool)>,
    ),
    Error,
> {
    let mut values: HashMap<&str, i32> = HashMap::new();

    for &(name, lo, hi) in SINT32 {
        exe.seek(lo)?;
        let a = exe.u16()? as i32;
        exe.seek(hi)?;
        let b = exe.u16()? as i16 as i32;
        values.insert(name, a.wrapping_add(b << 16));
    }
    for &(name, lo, hi) in SINT24 {
        exe.seek(lo)?;
        let a = exe.u16()? as i32;
        exe.seek(hi)?;
        let b = exe.u8()? as i8 as i32;
        values.insert(name, a.wrapping_add(b << 16));
    }
    for &(name, at) in SINT16 {
        exe.seek(at)?;
        values.insert(name, exe.u16()? as i16 as i32);
    }
    for &(name, at) in UINT16 {
        exe.seek(at)?;
        values.insert(name, exe.u16()? as i32);
    }
    for &(name, at) in SINT8 {
        exe.seek(at)?;
        values.insert(name, exe.u8()? as i8 as i32);
    }
    for &(name, at) in UINT8 {
        exe.seek(at)?;
        values.insert(name, exe.u8()? as i32);
    }

    let constants = CONSTANT_ORDER
        .iter()
        .map(|&name| (name, values.get(name).copied().unwrap_or(0)))
        .collect();

    let mut texts = Vec::with_capacity(TEXTS.len());
    for &(name, at) in TEXTS {
        exe.seek(at)?;
        texts.push((name, exe.pascal_string()?));
    }

    let mut hacks = Vec::with_capacity(HACKS.len());
    for &(name, indicators) in HACKS {
        let mut active = true;
        for &(at, byte) in indicators {
            exe.seek(at)?;
            if exe.u8()? != byte {
                active = false;
                break;
            }
        }
        hacks.push((name, active));
    }

    Ok((constants, texts, hacks))
}

fn load_font(exe: R) -> Result<Vec<FontChar>, Error> {
    exe.seek(0x1C825)?;
    let raw = exe.bytes(250 * 64 + 1)?;

    Ok((0..250)
        .map(|i| {
            let ptr = &raw[i * 64 + 1..i * 64 + 65];
            let mut data = [0u8; 56];
            for y in 0..8 {
                for x in 0..7 {
                    data[y * 7 + x] = ptr[y * 8 + x];
                }
            }
            FontChar {
                data,
                width: ptr[63] as usize,
            }
        })
        .collect())
}

/// What `load_palette` reads.
type Palette = ([[u8; 3]; 256], [(i32, i32); 4]);

fn load_palette(exe: R) -> Result<Palette, Error> {
    exe.seek(132774)?;
    let mut palette = [[0u8; 3]; 256];
    for entry in palette.iter_mut() {
        let rgb = exe.bytes(3)?;
        *entry = [rgb[0] & 63, rgb[1] & 63, rgb[2] & 63];
    }

    exe.seek(0x1AF0C)?;
    let mut color_anim = [(0, 0); 4];
    for anim in color_anim.iter_mut() {
        *anim = (r8(exe)?, r8(exe)?);
    }

    Ok((palette, color_anim))
}

fn load_materials(exe: R) -> Result<[u8; 256], Error> {
    let mut materials = [0u8; 256];

    let mut add_bits = |bits: &[u8], flag: u32| {
        for (j, m) in materials.iter_mut().enumerate() {
            *m |= ((bits[j >> 3] >> (j & 7)) & 1) << flag;
        }
    };

    exe.seek(0x01C2E0)?;
    for flag in 0..5 {
        add_bits(exe.bytes(32)?, flag);
    }

    exe.seek(0x01AEA8)?;
    add_bits(exe.bytes(32)?, 5);

    Ok(materials)
}

type Objects = (Vec<Weapon>, Vec<SObject>, Vec<NObject>);

fn load_objects(exe: R) -> Result<Objects, Error> {
    let mut w: Vec<Weapon> = (0..40).map(|_| Weapon::default()).collect();

    exe.seek(112806)?;
    column!(exe, w, r8, detect_distance);
    column!(exe, w, rbool, affect_by_worm);
    column!(exe, w, r8, blow_away);

    exe.seek(112966)?;
    column!(exe, w, r16, gravity);
    column!(exe, w, rbool, shadow);
    column!(exe, w, rbool, laser_sight);
    column!(exe, w, rdec8, launch_sound);
    column!(exe, w, rbool_i32, loop_sound);
    column!(exe, w, rdec8, explo_sound);
    column!(exe, w, r16, speed);
    column!(exe, w, r16, add_speed);
    column!(exe, w, r16, distribution);
    column!(exe, w, r8, parts);
    column!(exe, w, r8, recoil);
    column!(exe, w, r16, mult_speed);
    column!(exe, w, r16, delay);
    column!(exe, w, r16, loading_time);
    column!(exe, w, r8, ammo);
    column!(exe, w, rdec8, create_on_exp);
    column!(exe, w, rdec8, dirt_effect);
    column!(exe, w, r8, leave_shells);
    column!(exe, w, r8, leave_shell_delay);
    column!(exe, w, rbool, play_reload_sound);
    column!(exe, w, rbool, worm_explode);
    column!(exe, w, rbool, expl_ground);
    column!(exe, w, rbool, worm_collide);
    column!(exe, w, r8, fire_cone);
    column!(exe, w, rbool, collide_with_objects);
    column!(exe, w, rbool, affect_by_explosions);
    column!(exe, w, r8, bounce);
    column!(exe, w, r16, time_to_explo);
    column!(exe, w, r16, time_to_explo_v);
    column!(exe, w, r8, hit_damage);
    column!(exe, w, r8, blood_on_hit);
    column!(exe, w, r16, start_frame);
    column!(exe, w, r8, num_frames);
    column!(exe, w, rbool, loop_anim);
    column!(exe, w, r8, shot_type);
    column!(exe, w, r8, color_bullets);
    column!(exe, w, r8, splinter_amount);
    column!(exe, w, r8, splinter_colour);
    column!(exe, w, rdec8, splinter_type);
    column!(exe, w, r8, splinter_scatter);
    column!(exe, w, rdec8, obj_trail_type);
    column!(exe, w, r8, obj_trail_delay);
    column!(exe, w, r8, part_trail_type);
    column!(exe, w, rdec8, part_trail_obj);
    column!(exe, w, r8, part_trail_delay);

    exe.seek(0x1B676)?;
    for (i, weapon) in w.iter_mut().enumerate() {
        weapon.name = exe.pascal_string_field(14)?;
        weapon.id_str = to_id(&weapon.name);
        weapon.chain_explosion = i == 34;
    }

    let mut s: Vec<SObject> = SOBJECT_NAMES
        .iter()
        .map(|name| SObject {
            id_str: to_id(name.as_bytes()),
            ..Default::default()
        })
        .collect();

    exe.seek(115218)?;
    column!(exe, s, rdec8, start_sound);
    column!(exe, s, r8, num_sounds);
    column!(exe, s, r8, anim_delay);
    column!(exe, s, r8, start_frame);
    column!(exe, s, r8, num_frames);
    column!(exe, s, r8, detect_range);
    column!(exe, s, r8, damage);
    // blowAway has 13 slots, not 14. The last value overlaps with shadow.
    column!(exe, s, r32, blow_away);

    exe.seek(115368)?;
    column!(exe, s, rbool, shadow);
    column!(exe, s, r8, shake);
    column!(exe, s, r8, flash);
    column!(exe, s, rdec8, dirt_effect);

    let mut n: Vec<NObject> = NOBJECT_NAMES
        .iter()
        .map(|name| NObject {
            id_str: to_id(name.as_bytes()),
            ..Default::default()
        })
        .collect();

    exe.seek(111430)?;
    column!(exe, n, r8, detect_distance);
    column!(exe, n, r16, gravity);
    column!(exe, n, r16, speed);
    column!(exe, n, r16, speed_v);
    column!(exe, n, r16, distribution);
    column!(exe, n, r8, blow_away);
    column!(exe, n, r8, bounce);
    column!(exe, n, r8, hit_damage);
    column!(exe, n, rbool, worm_explode);
    column!(exe, n, rbool, expl_ground);
    column!(exe, n, rbool, worm_destroy);
    column!(exe, n, r8, blood_on_hit);
    column!(exe, n, r8, start_frame);
    column!(exe, n, r8, num_frames);
    column!(exe, n, rbool, draw_on_map);
    column!(exe, n, r8, color_bullets);
    column!(exe, n, rdec8, create_on_exp);
    column!(exe, n, rbool, affect_by_explosions);
    column!(exe, n, rdec8, dirt_effect);
    column!(exe, n, r8, splinter_amount);
    column!(exe, n, r8, splinter_colour);
    column!(exe, n, rdec8, splinter_type);
    column!(exe, n, rbool, blood_trail);
    column!(exe, n, r8, blood_trail_delay);
    column!(exe, n, rdec8, leave_obj);
    column!(exe, n, r8, leave_obj_delay);
    column!(exe, n, r16, time_to_explo);
    column!(exe, n, r16, time_to_explo_v);

    Ok((w, s, n))
}

fn load_textures(exe: R) -> Result<[Texture; 9], Error> {
    let mut t = [Texture::default(); 9];
    exe.seek(0x1C208)?;
    column!(exe, t, rbool, n_draw_back);
    exe.seek(0x1C1EA)?;
    column!(exe, t, r8, m_frame);
    exe.seek(0x1C1F4)?;
    column!(exe, t, r8, s_frame);
    exe.seek(0x1C1FE)?;
    column!(exe, t, r8, r_frame);
    Ok(t)
}

/// Sprites are stored column by column in LIERO.CHR; this turns them row-major.
fn read_sprite_set(chr: R, size: usize, count: usize) -> Result<SpriteSet, Error> {
    let raw = chr.bytes(size * size * count)?;
    let mut data = vec![0u8; raw.len()];

    for i in 0..count {
        let base = i * size * size;
        for x in 0..size {
            for y in 0..size {
                data[base + x + y * size] = raw[base + x * size + y];
            }
        }
    }

    Ok(SpriteSet {
        width: size,
        height: size,
        count,
        data,
    })
}

fn load_sprites(chr: R) -> Result<(SpriteSet, SpriteSet, SpriteSet), Error> {
    chr.seek(10)?; // Skip some header

    let mut large = read_sprite_set(chr, 16, 110)?;
    chr.skip(4)?; // Extra stuff
    let small = read_sprite_set(chr, 7, 130)?;
    chr.skip(4)?; // Extra stuff
    let text = read_sprite_set(chr, 4, 26)?;

    // The original only rendered 10x9 pixels of the worm sprites, so crop them to match.
    for frame in 16..16 + 21 {
        let sprite = large.sprite_mut(frame);
        for y in 0..16 {
            for x in 0..16 {
                if !(2..12).contains(&x) || y >= 9 {
                    sprite[y * 16 + x] = 0;
                }
            }
        }
    }

    // Fill some frames with noise, with the same generator and order as the C++ tctool.
    let mut rand = Mwc::new();
    for idx in 0..16 * 16 {
        for (frame, base) in [(73, 160), (74, 160), (87, 12), (88, 12), (82, 94), (83, 94)] {
            large.sprite_mut(frame)[idx] = (rand.below(4) + base) as u8;
        }
    }

    Ok((large, small, text))
}

fn load_sounds(snd: R) -> Result<Vec<Sound>, Error> {
    let count = snd.u16()?;
    let mut sounds = Vec::with_capacity(count as usize);

    for _ in 0..count {
        let raw_name = snd.bytes(8)?;
        let name_len = raw_name.iter().position(|&c| c == 0).unwrap_or(8);
        let name = to_id(&raw_name[..name_len]);

        let offset = snd.u32()? as usize;
        let length = snd.u32()? as usize;

        let mut data = Vec::new();
        if length > 0 {
            let entry_end = snd.pos();
            snd.seek(offset)?;
            data = snd.bytes(length)?.to_vec();
            snd.seek(entry_end)?;
        }

        sounds.push(Sound { name, data: Some(data) });
    }

    Ok(sounds)
}
