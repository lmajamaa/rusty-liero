//! Sound: the samples made from a TC's 8-bit sounds and the mixer that plays
//! them (formerly `mixer/mixer.c` and `SfxSample::createSound`).
//!
//! The mixer has 8 channels and makes mono signed 16-bit samples. A sound is
//! added at a frame time with an id (any pointer, only compared) and plays
//! one sample per frame from that time, at a 4.12 fixed-point volume, until
//! it ends or is stopped; a looping sound starts over. `liero_mixer_mix`
//! fills a buffer and advances the mixer's time; the web shell's audio node
//! calls it (`web.rs`).
//!
//! The C mixer was written so a game thread could add sounds while an audio
//! thread mixed. The game only builds for wasm without threads, where the
//! audio node runs on the page's thread, so here the two never overlap.
//!
//! Quirks kept from the C mixer:
//! - Stopping a channel doesn't clear its id, so `liero_mixer_is_playing`
//!   keeps finding a stopped sound until its channel is reused. (A looping
//!   weapon sound that was stopped isn't restarted until then.)
//! - Ids are looked up by the first channel with that id, active or not,
//!   so the null id is "playing" whenever any channel has never been used
//!   or has finished, and stopping the null id stops the first such
//!   channel.
//! - A sound added again with an id already playing takes another channel.
//! - A sound added at a time already past starts from its beginning now.
//! - Volume is `volume * 4096` truncated and masked to 13 bits, so 2.0 is
//!   silent.

use std::ffi::c_void;

/// Number of channels; more sounds than this at once aren't played.
pub const LIERO_MIXER_CHANNELS: usize = 8;
/// Flags for `liero_mixer_add`.
pub const LIERO_SOUND_NORMAL: u32 = 0;
pub const LIERO_SOUND_LOOP: u32 = 1;

const INACTIVE: u32 = 0xffff_ffff;

/// The game's samples for an 8-bit TC sound: each byte times 30, with the
/// average of each pair put between them and the last one repeated, so twice
/// as many samples.
pub fn samples_from_tc(data: &[u8], out: &mut [i16]) {
    assert_eq!(out.len(), data.len() * 2);
    let Some((&first, rest)) = data.split_first() else {
        return;
    };
    let mut prev = (first as i8) as i32 * 30;
    out[0] = prev as i16;
    for (j, &b) in rest.iter().enumerate() {
        let cur = (b as i8) as i32 * 30;
        out[1 + 2 * j] = ((prev + cur) / 2) as i16;
        out[2 + 2 * j] = cur as i16;
        prev = cur;
    }
    out[out.len() - 1] = prev as i16;
}

#[derive(Clone, Copy, Debug)]
struct Channel {
    id: usize,
    flags: u32,
    /// The samples, owned by C++ (`SfxSample`).
    sound: *const i16,
    len: usize,
    /// Frame time it starts at.
    pos: u32,
    /// Position in the sound, 32.32 fixed point.
    sound_pos: u64,
    stride: u64,
    /// Two copies of the 4.12 volume, as the C kept for stereo; only the
    /// low 13 bits are used.
    volumes: u32,
}

impl Default for Channel {
    fn default() -> Self {
        Channel {
            id: 0,
            flags: INACTIVE,
            sound: std::ptr::null(),
            len: 0,
            pos: 0,
            sound_pos: 0,
            stride: 0,
            volumes: 0,
        }
    }
}

impl Channel {
    /// Adds this channel to `out`, which starts at frame `now`. Returns
    /// false once the sound has ended.
    fn add_to(&mut self, out: &mut [i16], now: u32, sound: &[i16]) -> bool {
        let rel_begin = (self.pos.wrapping_sub(now) as i32).max(0) as usize;
        let len = sound.len() as u32;
        let scaler = (self.volumes & 0x1fff) as i32;
        for o in out.iter_mut().skip(rel_begin) {
            // An empty sound has nothing to read; C read past its end.
            let s = sound.get((self.sound_pos >> 32) as usize).copied().unwrap_or(0) as i32;
            let mixed = (*o as i32 + ((s * scaler) >> 12)).clamp(-32768, 32767);
            *o = mixed as i16;
            self.sound_pos = self.sound_pos.wrapping_add(self.stride);
            if (self.sound_pos >> 32) as u32 >= len {
                if self.flags & LIERO_SOUND_LOOP != 0 {
                    self.sound_pos = 0;
                } else {
                    return false;
                }
            }
        }
        true
    }
}

/// The mixer's channels and time. Opaque to C++.
#[derive(Clone, Debug, Default)]
pub struct LieroMixer {
    channels: [Channel; LIERO_MIXER_CHANNELS],
    base_frame: i32,
}

impl LieroMixer {
    pub fn now(&self) -> i32 {
        self.base_frame
    }

    fn find(&self, id: usize) -> Option<usize> {
        self.channels.iter().position(|c| c.id == id)
    }

    /// Plays `sound` from frame `time` on the first free channel. Returns
    /// false if every channel is busy.
    ///
    /// # Safety
    /// `sound` must point to `len` samples (or be anything with `len` 0)
    /// that stay alive while the channel plays them.
    pub unsafe fn add(&mut self, sound: *const i16, len: usize, time: u32, id: usize, flags: u32) -> bool {
        match self.channels.iter().position(|c| c.flags == INACTIVE) {
            Some(i) => {
                self.channels[i] = Channel {
                    id,
                    flags,
                    sound,
                    len,
                    pos: time,
                    sound_pos: 0,
                    stride: 1 << 32,
                    volumes: 0x1000_1000,
                };
                true
            }
            None => false,
        }
    }

    pub fn stop(&mut self, id: usize) {
        if let Some(i) = self.find(id) {
            self.channels[i].flags = INACTIVE;
        }
    }

    pub fn is_playing(&self, id: usize) -> bool {
        self.find(id).is_some()
    }

    pub fn set_volume(&mut self, id: usize, volume: f64) {
        if let Some(i) = self.find(id) {
            // C converted with (uint32_t); this saturates where that was
            // undefined (negative or huge volumes).
            let v = (volume * 4096.0) as u32;
            self.channels[i].volumes = (v << 16).wrapping_add(v);
        }
    }

    /// Mixes the next `out.len()` frames into `out`.
    pub fn mix(&mut self, out: &mut [i16]) {
        out.fill(0);
        let now = self.base_frame as u32;
        for ch in self.channels.iter_mut() {
            if ch.flags == INACTIVE {
                continue;
            }
            // SAFETY: `add` requires the samples to outlive the channel.
            let sound: &[i16] = if ch.len == 0 {
                &[]
            } else {
                unsafe { std::slice::from_raw_parts(ch.sound, ch.len) }
            };
            if !ch.add_to(out, now, sound) {
                ch.flags = INACTIVE;
                ch.id = 0;
            }
        }
        self.base_frame = self.base_frame.wrapping_add(out.len() as i32);
    }
}

/// Fills `out` with the `2 * len` samples of an 8-bit TC sound of `len`
/// bytes (`SfxSample::createSound`).
///
/// # Safety
/// `data` must point to `len` bytes and `out` to `2 * len` samples, or both
/// be anything with `len` 0.
#[no_mangle]
pub unsafe extern "C" fn liero_sound_from_tc(data: *const u8, len: usize, out: *mut i16) {
    unsafe {
        if len == 0 {
            return;
        }
        samples_from_tc(
            std::slice::from_raw_parts(data, len),
            std::slice::from_raw_parts_mut(out, len * 2),
        );
    }
}

/// A mixer with every channel free. Free it with `liero_mixer_free`.
/// Where a game's sounds go: a mixer, or none (`SoundPlayer`), and a
/// function told of each sound started (the hash run's app driver).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroSoundOut {
    pub mixer: *mut LieroMixer,
    pub report: Option<unsafe extern "C" fn(sound: i32)>,
}

impl LieroSoundOut {
    pub const NONE: LieroSoundOut = LieroSoundOut {
        mixer: std::ptr::null_mut(),
        report: None,
    };

    /// Plays the TC's `sound` as `id`'s, at the mixer's time, looping when
    /// `loops` isn't 0 (`playOnMixer`).
    ///
    /// # Safety
    /// The mixer, if any, is valid, and `sound` is one of `tc`'s.
    pub unsafe fn play(&self, tc: &crate::tc::LieroTcData, sound: i32, id: *mut c_void, loops: i32) {
        unsafe {
            if let Some(report) = self.report {
                report(sound);
            }
            if let Some(m) = self.mixer.as_mut() {
                assert!((sound as usize) < tc.sound_count);
                let s = &*tc.sounds.add(sound as usize);
                let now = liero_mixer_now(m) as u32;
                liero_mixer_add(
                    m,
                    s.samples,
                    s.sample_count,
                    now,
                    id,
                    if loops != 0 {
                        LIERO_SOUND_LOOP
                    } else {
                        LIERO_SOUND_NORMAL
                    },
                );
            }
        }
    }

    /// # Safety
    /// The mixer, if any, is valid.
    pub unsafe fn playing(&self, id: *mut c_void) -> bool {
        unsafe { self.mixer.as_ref().is_some_and(|m| liero_mixer_is_playing(m, id)) }
    }

    /// # Safety
    /// The mixer, if any, is valid.
    pub unsafe fn stop(&self, id: *mut c_void) {
        unsafe {
            if let Some(m) = self.mixer.as_mut() {
                liero_mixer_stop(m, id);
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn liero_mixer_new() -> *mut LieroMixer {
    Box::into_raw(Box::default())
}

/// # Safety
/// `m` must come from `liero_mixer_new`, or be null.
#[no_mangle]
pub unsafe extern "C" fn liero_mixer_free(m: *mut LieroMixer) {
    unsafe {
        if !m.is_null() {
            drop(Box::from_raw(m));
        }
    }
}

/// The frame time the next mix starts at.
#[no_mangle]
pub extern "C" fn liero_mixer_now(m: &LieroMixer) -> i32 {
    m.now()
}

/// Plays the `len` samples at `samples` from frame `time` with `id` and
/// `flags` (`LIERO_SOUND_LOOP`). Returns `id`, or null when every channel
/// is busy.
///
/// # Safety
/// `samples` must point to `len` samples (or be anything with `len` 0)
/// that stay alive while the mixer plays them.
#[no_mangle]
pub unsafe extern "C" fn liero_mixer_add(
    m: &mut LieroMixer,
    samples: *const i16,
    len: usize,
    time: u32,
    id: *mut c_void,
    flags: u32,
) -> *mut c_void {
    unsafe {
        if m.add(samples, len, time, id as usize, flags) {
            id
        } else {
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn liero_mixer_stop(m: &mut LieroMixer, id: *mut c_void) {
    m.stop(id as usize);
}

#[no_mangle]
pub extern "C" fn liero_mixer_is_playing(m: &LieroMixer, id: *mut c_void) -> bool {
    m.is_playing(id as usize)
}

#[no_mangle]
pub extern "C" fn liero_mixer_set_volume(m: &mut LieroMixer, id: *mut c_void, volume: f64) {
    m.set_volume(id as usize, volume);
}

/// Mixes the next `frames` frames into `out`.
///
/// # Safety
/// `out` must point to `frames` samples.
#[no_mangle]
pub unsafe extern "C" fn liero_mixer_mix(m: &mut LieroMixer, out: *mut i16, frames: usize) {
    unsafe {
        if frames == 0 {
            return;
        }
        m.mix(std::slice::from_raw_parts_mut(out, frames));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sound(s: &[i16]) -> Vec<i16> {
        s.to_vec()
    }

    fn add(m: &mut LieroMixer, s: &[i16], time: u32, id: usize, flags: u32) -> bool {
        unsafe { m.add(s.as_ptr(), s.len(), time, id, flags) }
    }

    fn from_tc(data: &[u8]) -> Vec<i16> {
        let mut out = vec![0; data.len() * 2];
        samples_from_tc(data, &mut out);
        out
    }

    fn mix(m: &mut LieroMixer, n: usize) -> Vec<i16> {
        let mut out = vec![1234; n];
        m.mix(&mut out);
        out
    }

    #[test]
    fn tc_samples() {
        assert_eq!(from_tc(&[]), Vec::<i16>::new());
        assert_eq!(from_tc(&[5]), vec![150, 150]);
        // -128 * 30 = -3840; (150 + -3840) / 2 truncates towards zero.
        assert_eq!(from_tc(&[5, 0x80, 0x7f]), vec![150, -1845, -3840, -15, 3810, 3810]);
        assert_eq!(from_tc(&[1, 2]), vec![30, 45, 60, 60]);
        assert_eq!(from_tc(&[0xff, 0]), vec![-30, -15, 0, 0]);
    }

    #[test]
    fn plays_once_at_its_time() {
        let s = sound(&[100, 200, 300]);
        let mut m = LieroMixer::default();
        assert!(add(&mut m, &s, 2, 7, LIERO_SOUND_NORMAL));
        assert!(m.is_playing(7));
        assert_eq!(mix(&mut m, 4), vec![0, 0, 100, 200]);
        assert_eq!(m.now(), 4);
        assert!(m.is_playing(7));
        assert_eq!(mix(&mut m, 3), vec![300, 0, 0]);
        assert!(!m.is_playing(7));
    }

    #[test]
    fn past_start_plays_from_the_beginning() {
        let s = sound(&[1, 2]);
        let mut m = LieroMixer::default();
        mix(&mut m, 10);
        add(&mut m, &s, 3, 1, 0);
        assert_eq!(mix(&mut m, 3), vec![1, 2, 0]);
    }

    #[test]
    fn far_future_waits() {
        let s = sound(&[9]);
        let mut m = LieroMixer::default();
        add(&mut m, &s, 5, 1, 0);
        assert_eq!(mix(&mut m, 5), vec![0; 5]);
        assert_eq!(mix(&mut m, 2), vec![9, 0]);
    }

    #[test]
    fn loops() {
        let s = sound(&[1, 2, 3]);
        let mut m = LieroMixer::default();
        add(&mut m, &s, 0, 1, LIERO_SOUND_LOOP);
        assert_eq!(mix(&mut m, 7), vec![1, 2, 3, 1, 2, 3, 1]);
        assert_eq!(mix(&mut m, 2), vec![2, 3]);
        m.stop(1);
        assert_eq!(mix(&mut m, 2), vec![0, 0]);
    }

    #[test]
    fn sound_out_stops_a_loop() {
        let s = sound(&[1, 2, 3]);
        let mut m = LieroMixer::default();
        add(&mut m, &s, 0, 1, LIERO_SOUND_LOOP);
        let out = LieroSoundOut {
            mixer: &mut m,
            report: None,
        };
        unsafe {
            assert!(out.playing(1 as *mut c_void));
            out.stop(1 as *mut c_void);
        }
        assert_eq!(mix(&mut m, 2), vec![0, 0]);
        unsafe { LieroSoundOut::NONE.stop(1 as *mut c_void) };
        assert!(unsafe { !LieroSoundOut::NONE.playing(1 as *mut c_void) });
    }

    #[test]
    fn stopped_id_stays_until_reused() {
        let s = sound(&[1; 100]);
        let mut m = LieroMixer::default();
        add(&mut m, &s, 0, 5, LIERO_SOUND_LOOP);
        m.stop(5);
        mix(&mut m, 10);
        assert!(m.is_playing(5));
        add(&mut m, &s, 0, 6, 0);
        assert!(!m.is_playing(5));
        assert!(m.is_playing(6));
    }

    #[test]
    fn null_id_matches_unused_channels() {
        let s = sound(&[1; 4]);
        let mut m = LieroMixer::default();
        assert!(m.is_playing(0));
        for id in 1..=8 {
            add(&mut m, &s, 0, id, 0);
        }
        assert!(!m.is_playing(0));
        // Every channel is busy.
        assert!(!add(&mut m, &s, 0, 9, 0));
        mix(&mut m, 4);
        assert!(m.is_playing(0));
        assert!(!m.is_playing(1));
    }

    #[test]
    fn repeated_id_takes_another_channel() {
        let a = sound(&[10; 2]);
        let b = sound(&[1; 4]);
        let mut m = LieroMixer::default();
        add(&mut m, &a, 0, 3, 0);
        add(&mut m, &b, 0, 3, 0);
        // Volume and stop act on the first.
        m.set_volume(3, 0.5);
        assert_eq!(mix(&mut m, 4), vec![6, 6, 1, 1]);
    }

    #[test]
    fn volume() {
        let s = sound(&[1000, -1000, 32767, -32768]);
        let mut m = LieroMixer::default();
        for (v, want) in [
            (1.0, [1000, -1000, 32767, -32768]),
            (0.5, [500, -500, 16383, -16384]),
            (0.25, [250, -250, 8191, -8192]),
            (0.0, [0, 0, 0, 0]),
            // Arithmetic shift rounds negative samples down.
            (0.001, [0, -1, 31, -32]),
            // 13 bits: 1.999 is nearly double, 2.0 silent, 2.5 a half.
            (1.999, [1998, -1999, 32767, -32768]),
            (2.0, [0, 0, 0, 0]),
            (2.5, [500, -500, 16383, -16384]),
        ] {
            let now = m.now() as u32;
            add(&mut m, &s, now, 1, 0);
            m.set_volume(1, v);
            assert_eq!(mix(&mut m, 4), want, "volume {v}");
        }
    }

    #[test]
    fn clips() {
        let s = sound(&[30000, -30000, 20000]);
        let mut m = LieroMixer::default();
        add(&mut m, &s, 0, 1, 0);
        add(&mut m, &s, 0, 2, 0);
        add(&mut m, &s, 2, 3, 0);
        assert_eq!(mix(&mut m, 5), vec![32767, -32768, 32767, -30000, 20000]);
    }

    #[test]
    fn empty_sounds() {
        // An empty sound plays one silent frame. (C read a sample past its
        // end, and crashed on a TC sound that failed to load.)
        let mut m = LieroMixer::default();
        add(&mut m, &[], 0, 1, 0);
        unsafe {
            assert_eq!(
                liero_mixer_add(&mut m, std::ptr::null(), 0, 0, 2 as *mut c_void, LIERO_SOUND_LOOP),
                2 as *mut c_void
            );
        }
        assert_eq!(mix(&mut m, 2), vec![0, 0]);
        assert!(!m.is_playing(1));
        // Looping, it keeps its channel.
        assert!(m.is_playing(2));
    }

    #[test]
    fn time_wraps() {
        let s = sound(&[7, 8]);
        let mut m = LieroMixer {
            base_frame: i32::MAX - 1,
            ..Default::default()
        };
        add(&mut m, &s, (i32::MAX as u32) + 1, 1, 0);
        assert_eq!(mix(&mut m, 4), vec![0, 0, 7, 8]);
        assert_eq!(m.now(), i32::MIN + 2);
    }

    #[test]
    fn ffi() {
        unsafe {
            let mut s = [0i16; 4];
            liero_sound_from_tc([1u8, 2].as_ptr(), 2, s.as_mut_ptr());
            assert_eq!(s, [30, 45, 60, 60]);
            s[0] = 99;
            liero_sound_from_tc(std::ptr::null(), 0, std::ptr::null_mut());
            let m = liero_mixer_new();
            let id = 42 as *mut c_void;
            assert_eq!(liero_mixer_add(&mut *m, s.as_ptr(), 4, 0, id, LIERO_SOUND_NORMAL), id);
            assert!(liero_mixer_is_playing(&*m, id));
            let mut out = [0i16; 5];
            liero_mixer_mix(&mut *m, out.as_mut_ptr(), 5);
            assert_eq!(out, [99, 45, 60, 60, 0]);
            assert_eq!(liero_mixer_now(&*m), 5);
            liero_mixer_free(m);
        }
    }
}
