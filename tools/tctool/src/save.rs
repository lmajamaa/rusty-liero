//! Writes a TC folder. Port of `src/tc_tool/common_writer.cpp` and the
//! `archive_text` functions in `src/game/common_model.hpp`.

use crate::model::*;
use crate::toml::Writer;
use crate::Error;
use std::fs;
use std::path::Path;

pub fn save(common: &Common, path: &Path) -> Result<(), Error> {
    fs::create_dir_all(path)?;
    write(&path.join("tc.cfg"), &tc_cfg(common))?;

    if !common.sounds.is_empty() {
        let dir = path.join("sounds");
        fs::create_dir_all(&dir)?;
        for sound in &common.sounds {
            write(&dir.join(format!("{}.wav", sound.name)), &wav(&sound.data))?;
        }
    }

    let dir = path.join("sprites");
    fs::create_dir_all(&dir)?;
    for (name, sprites) in [
        ("small.tga", &common.small_sprites),
        ("large.tga", &common.large_sprites),
        ("text.tga", &common.text_sprites),
    ] {
        let tga = tga(sprites.width, sprites.count * sprites.height, &sprites.data, &common.palette);
        write(&dir.join(name), &tga)?;
    }
    write(&dir.join("font.tga"), &font_tga(common))?;

    let ids = Ids::new(common);

    let dir = path.join("weapons");
    fs::create_dir_all(&dir)?;
    for w in &common.weapons {
        write(&dir.join(format!("{}.cfg", w.id_str)), &weapon_cfg(w, &ids))?;
    }

    let dir = path.join("nobjects");
    fs::create_dir_all(&dir)?;
    for n in &common.nobjects {
        write(&dir.join(format!("{}.cfg", n.id_str)), &nobject_cfg(n, &ids))?;
    }

    let dir = path.join("sobjects");
    fs::create_dir_all(&dir)?;
    for s in &common.sobjects {
        write(&dir.join(format!("{}.cfg", s.id_str)), &sobject_cfg(s))?;
    }

    Ok(())
}

fn write(path: &Path, data: &[u8]) -> Result<(), Error> {
    fs::write(path, data).map_err(|e| Error(format!("Could not write {}: {}", path.display(), e)))
}

/// Object ids, for writing references between objects.
struct Ids<'a> {
    nobjects: Vec<&'a str>,
    sobjects: Vec<&'a str>,
}

impl<'a> Ids<'a> {
    fn new(common: &'a Common) -> Self {
        Ids {
            nobjects: common.nobjects.iter().map(|n| n.id_str.as_str()).collect(),
            sobjects: common.sobjects.iter().map(|s| s.id_str.as_str()).collect(),
        }
    }
}

fn tc_cfg(common: &Common) -> Vec<u8> {
    let ids = Ids::new(common);
    let mut w = Writer::new();

    w.obj(Some("types"), |w| {
        w.arr("sounds", &common.sounds, |w, s| w.str_value(s.name.as_bytes()));
        w.arr("weapons", &common.weapons, |w, x| w.str_value(x.id_str.as_bytes()));
        w.arr("nobjects", &common.nobjects, |w, x| w.str_value(x.id_str.as_bytes()));
        w.arr("sobjects", &common.sobjects, |w, x| w.str_value(x.id_str.as_bytes()));
    });

    w.obj(Some("constants"), |w| {
        w.array_obj("bonuses", &[0usize, 1], |w, &i| {
            w.i32("timer", common.bonus_rand_timer[i][0]);
            w.i32("timerV", common.bonus_rand_timer[i][1]);
            w.i32("frame", common.bonus_frames[i]);
            w.reference("sobj", common.bonus_sobjects[i], &ids.sobjects);
        });

        w.array_obj("textures", &common.textures, |w, t| {
            w.i32("mframe", t.m_frame);
            w.i32("rframe", t.r_frame);
            w.i32("sframe", t.s_frame);
            w.bool("ndrawback", t.n_draw_back);
        });

        w.array_obj("colorAnim", &common.color_anim, |w, &(from, to)| {
            w.i32("from", from);
            w.i32("to", to);
        });

        w.arr("materials", &common.materials, |w, &m| w.i32_value(m as i32));

        const KEYS: [&str; 7] = ["up", "down", "left", "right", "fire", "change", "jump"];
        w.obj(Some("aiparams"), |w| {
            for (i, key) in KEYS.into_iter().enumerate() {
                w.obj(Some(key), |w| {
                    w.i32("on", common.ai_params[1][i]);
                    w.i32("off", common.ai_params[0][i]);
                });
            }
        });

        for &(name, value) in &common.constants {
            w.i32(name, value);
        }
    });

    w.obj(Some("texts"), |w| {
        for (name, text) in &common.texts {
            w.str(name, text);
        }
    });

    w.obj(Some("hacks"), |w| {
        for &(name, active) in &common.hacks {
            w.bool(name, active);
        }
    });

    w.finish()
}

fn weapon_cfg(x: &Weapon, ids: &Ids) -> Vec<u8> {
    let mut w = Writer::new();
    w.obj(None, |w| {
        w.str("name", &x.name);

        w.bool("affectByWorm", x.affect_by_worm);
        w.bool("shadow", x.shadow);
        w.bool("laserSight", x.laser_sight);
        w.bool("playReloadSound", x.play_reload_sound);
        w.bool("wormExplode", x.worm_explode);
        w.bool("explGround", x.expl_ground);
        w.bool("wormCollide", x.worm_collide);
        w.bool("collideWithObjects", x.collide_with_objects);
        w.bool("affectByExplosions", x.affect_by_explosions);
        w.bool("loopAnim", x.loop_anim);
        w.i32("detectDistance", x.detect_distance);
        w.i32("blowAway", x.blow_away);
        w.i32("gravity", x.gravity);
        w.i32("launchSound", x.launch_sound);
        w.i32("loopSound", x.loop_sound);
        w.i32("exploSound", x.explo_sound);
        w.i32("speed", x.speed);
        w.i32("addSpeed", x.add_speed);
        w.i32("distribution", x.distribution);
        w.i32("parts", x.parts);
        w.i32("recoil", x.recoil);
        w.i32("multSpeed", x.mult_speed);
        w.i32("delay", x.delay);
        w.i32("loadingTime", x.loading_time);
        w.i32("ammo", x.ammo);
        w.i32("dirtEffect", x.dirt_effect);
        w.i32("leaveShells", x.leave_shells);
        w.i32("leaveShellDelay", x.leave_shell_delay);
        w.i32("fireCone", x.fire_cone);
        w.i32("bounce", x.bounce);
        w.i32("timeToExplo", x.time_to_explo);
        w.i32("timeToExploV", x.time_to_explo_v);
        w.i32("hitDamage", x.hit_damage);
        w.i32("bloodOnHit", x.blood_on_hit);
        w.i32("startFrame", x.start_frame);
        w.i32("numFrames", x.num_frames);
        w.i32("shotType", x.shot_type);
        w.i32("colorBullets", x.color_bullets);
        w.i32("splinterAmount", x.splinter_amount);
        w.i32("splinterColour", x.splinter_colour);
        w.reference("splinterType", x.splinter_type, &ids.nobjects);
        w.i32("splinterScatter", x.splinter_scatter);
        w.reference("objTrailType", x.obj_trail_type, &ids.sobjects);
        w.i32("objTrailDelay", x.obj_trail_delay);
        w.i32("partTrailType", x.part_trail_type);
        w.reference("partTrailObj", x.part_trail_obj, &ids.nobjects);
        w.i32("partTrailDelay", x.part_trail_delay);
        w.reference("createOnExp", x.create_on_exp, &ids.sobjects);
        w.bool("chainExplosion", x.chain_explosion);
    });
    w.finish()
}

fn nobject_cfg(x: &NObject, ids: &Ids) -> Vec<u8> {
    let mut w = Writer::new();
    w.obj(None, |w| {
        w.bool("wormExplode", x.worm_explode);
        w.bool("explGround", x.expl_ground);
        w.bool("wormDestroy", x.worm_destroy);
        w.bool("drawOnMap", x.draw_on_map);
        w.bool("affectByExplosions", x.affect_by_explosions);
        w.bool("bloodTrail", x.blood_trail);

        w.i32("detectDistance", x.detect_distance);
        w.i32("gravity", x.gravity);
        w.i32("speed", x.speed);
        w.i32("speedV", x.speed_v);
        w.i32("distribution", x.distribution);
        w.i32("blowAway", x.blow_away);
        w.i32("bounce", x.bounce);
        w.i32("hitDamage", x.hit_damage);
        w.i32("bloodOnHit", x.blood_on_hit);
        w.i32("startFrame", x.start_frame);
        w.i32("numFrames", x.num_frames);
        w.i32("colorBullets", x.color_bullets);
        w.reference("createOnExp", x.create_on_exp, &ids.sobjects);
        w.i32("dirtEffect", x.dirt_effect);
        w.i32("splinterAmount", x.splinter_amount);
        w.i32("splinterColour", x.splinter_colour);
        w.reference("splinterType", x.splinter_type, &ids.nobjects);
        w.i32("bloodTrailDelay", x.blood_trail_delay);
        w.reference("leaveObj", x.leave_obj, &ids.sobjects);
        w.i32("leaveObjDelay", x.leave_obj_delay);
        w.i32("timeToExplo", x.time_to_explo);
        w.i32("timeToExploV", x.time_to_explo_v);
    });
    w.finish()
}

fn sobject_cfg(x: &SObject) -> Vec<u8> {
    let mut w = Writer::new();
    w.obj(None, |w| {
        w.bool("shadow", x.shadow);
        w.i32("startSound", x.start_sound);
        w.i32("numSounds", x.num_sounds);
        w.i32("animDelay", x.anim_delay);
        w.i32("startFrame", x.start_frame);
        w.i32("numFrames", x.num_frames);
        w.i32("detectRange", x.detect_range);
        w.i32("damage", x.damage);
        w.i32("blowAway", x.blow_away);
        w.i32("shake", x.shake);
        w.i32("flash", x.flash);
        w.i32("dirtEffect", x.dirt_effect);
    });
    w.finish()
}

/// 8-bit mono 22050 Hz WAV. The samples are signed in LIERO.SND and unsigned in WAV.
fn wav(samples: &[u8]) -> Vec<u8> {
    let len = samples.len() as u32;
    let rounded = (len + 1) & !1;

    let mut out = Vec::with_capacity(44 + rounded as usize);
    out.extend_from_slice(b"RIFF");
    // The C++ tctool writes the padded data size minus 8 here; kept for identical output.
    out.extend_from_slice(&rounded.wrapping_sub(8).to_le_bytes());
    out.extend_from_slice(b"WAVE");

    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM header size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // Mono
    out.extend_from_slice(&22050u32.to_le_bytes()); // Sample rate
    out.extend_from_slice(&22050u32.to_le_bytes()); // Bytes per second
    out.extend_from_slice(&1u16.to_le_bytes()); // Block align
    out.extend_from_slice(&8u16.to_le_bytes()); // Bits per sample

    out.extend_from_slice(b"data");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend(samples.iter().map(|&s| s.wrapping_add(128)));
    out.resize(out.len() + (rounded - len) as usize, 0); // Padding

    out
}

/// Uncompressed 8-bit palettized TGA, rows stored bottom to top.
fn tga(width: usize, height: usize, pixels: &[u8], palette: &[[u8; 3]; 256]) -> Vec<u8> {
    let mut out = Vec::with_capacity(18 + 768 + width * height);
    out.extend_from_slice(&[0, 1, 1]); // No ID, has palette, palettized image
    out.extend_from_slice(&0u16.to_le_bytes()); // First palette entry
    out.extend_from_slice(&256u16.to_le_bytes()); // Palette size
    out.push(24); // Palette entry bits
    out.extend_from_slice(&0u16.to_le_bytes()); // X origin
    out.extend_from_slice(&0u16.to_le_bytes()); // Y origin
    out.extend_from_slice(&(width as u16).to_le_bytes());
    out.extend_from_slice(&(height as u16).to_le_bytes());
    out.push(8); // Bits per pixel
    out.push(0); // Descriptor

    for &[r, g, b] in palette {
        out.extend_from_slice(&[b << 2, g << 2, r << 2]);
    }

    for row in pixels[..width * height].chunks(width).rev() {
        out.extend_from_slice(row);
    }

    out
}

/// The font as a 7 pixel wide strip, 8 rows per character: 50 where the glyph is
/// set, 0 where it's clear, and 10 to the right of the character's width.
fn font_tga(common: &Common) -> Vec<u8> {
    let mut pixels = vec![10u8; common.font.len() * 7 * 8];

    for (i, ch) in common.font.iter().enumerate() {
        let dest = &mut pixels[i * 56..(i + 1) * 56];
        for y in 0..8 {
            for x in 0..ch.width.min(7) {
                dest[y * 7 + x] = if ch.data[y * 7 + x] != 0 { 50 } else { 0 };
            }
        }
    }

    tga(7, common.font.len() * 8, &pixels, &common.palette)
}
