//! `std::sort` as libc++ 19 (Emscripten 4.0.0's) does it for a custom
//! comparator. `std::sort` isn't stable, and the stats screen shows weapons
//! with equal damage in whatever order it leaves them, so matching the C++
//! means sorting the same way: introsort with a median-of-three pivot,
//! partitioning that keeps equal elements right of the pivot, insertion sort
//! below 24 elements, and heap sort when the depth runs out. Ported from
//! `__algorithm/sort.h` and the heap functions it uses; the bitset partition
//! and branchless sorts there are only for numbers with `<` or `>`.

/// Sorts `v` so that `less(a, b)` puts `a` before `b`, in the same order as
/// libc++'s `std::sort`.
pub fn sort<T: Copy>(v: &mut [T], less: &impl Fn(&T, &T) -> bool) {
    let n = v.len();
    let depth = if n == 0 { 0 } else { 2 * n.ilog2() as isize };
    introsort(v, 0, n, less, depth, true);
}

/// `std::partial_sort(first, last, last)`: heap sort, as `sort` falls back to.
pub fn heap_sort<T: Copy>(v: &mut [T], less: &impl Fn(&T, &T) -> bool) {
    make_heap(v, less);
    sort_heap(v, less);
}

fn sort3<T: Copy>(v: &mut [T], x: usize, y: usize, z: usize, c: &impl Fn(&T, &T) -> bool) {
    if !c(&v[y], &v[x]) {
        if !c(&v[z], &v[y]) {
            return;
        }
        v.swap(y, z);
        if c(&v[y], &v[x]) {
            v.swap(x, y);
        }
        return;
    }
    if c(&v[z], &v[y]) {
        v.swap(x, z);
        return;
    }
    v.swap(x, y);
    if c(&v[z], &v[y]) {
        v.swap(y, z);
    }
}

fn sort4<T: Copy>(v: &mut [T], x1: usize, x2: usize, x3: usize, x4: usize, c: &impl Fn(&T, &T) -> bool) {
    sort3(v, x1, x2, x3, c);
    if c(&v[x4], &v[x3]) {
        v.swap(x3, x4);
        if c(&v[x3], &v[x2]) {
            v.swap(x2, x3);
            if c(&v[x2], &v[x1]) {
                v.swap(x1, x2);
            }
        }
    }
}

fn sort5<T: Copy>(v: &mut [T], x: [usize; 5], c: &impl Fn(&T, &T) -> bool) {
    sort4(v, x[0], x[1], x[2], x[3], c);
    if c(&v[x[4]], &v[x[3]]) {
        v.swap(x[3], x[4]);
        if c(&v[x[3]], &v[x[2]]) {
            v.swap(x[2], x[3]);
            if c(&v[x[2]], &v[x[1]]) {
                v.swap(x[1], x[2]);
                if c(&v[x[1]], &v[x[0]]) {
                    v.swap(x[0], x[1]);
                }
            }
        }
    }
}

fn insertion_sort<T: Copy>(v: &mut [T], first: usize, last: usize, c: &impl Fn(&T, &T) -> bool) {
    if first == last {
        return;
    }
    for i in first + 1..last {
        let mut j = i - 1;
        if c(&v[i], &v[j]) {
            let t = v[i];
            let mut k = j;
            j = i;
            loop {
                v[j] = v[k];
                j = k;
                if j == first {
                    break;
                }
                k -= 1;
                if !c(&t, &v[k]) {
                    break;
                }
            }
            v[j] = t;
        }
    }
}

/// Insertion sort that relies on `v[first - 1]` being no greater than
/// anything in the range, instead of checking for the start.
fn insertion_sort_unguarded<T: Copy>(v: &mut [T], first: usize, last: usize, c: &impl Fn(&T, &T) -> bool) {
    if first == last {
        return;
    }
    for i in first + 1..last {
        let mut j = i - 1;
        if c(&v[i], &v[j]) {
            let t = v[i];
            let mut k = j;
            j = i;
            loop {
                v[j] = v[k];
                j = k;
                k -= 1;
                if !c(&t, &v[k]) {
                    break;
                }
            }
            v[j] = t;
        }
    }
}

/// Insertion sort that gives up after moving 8 elements. Returns whether
/// the range ended up sorted.
fn insertion_sort_incomplete<T: Copy>(v: &mut [T], first: usize, last: usize, c: &impl Fn(&T, &T) -> bool) -> bool {
    match last - first {
        0 | 1 => return true,
        2 => {
            if c(&v[last - 1], &v[first]) {
                v.swap(first, last - 1);
            }
            return true;
        }
        3 => {
            sort3(v, first, first + 1, last - 1, c);
            return true;
        }
        4 => {
            sort4(v, first, first + 1, first + 2, last - 1, c);
            return true;
        }
        5 => {
            sort5(v, [first, first + 1, first + 2, first + 3, last - 1], c);
            return true;
        }
        _ => {}
    }
    let mut j = first + 2;
    sort3(v, first, first + 1, j, c);
    let mut count = 0;
    let mut i = j + 1;
    while i != last {
        if c(&v[i], &v[j]) {
            let t = v[i];
            let mut k = j;
            j = i;
            loop {
                v[j] = v[k];
                j = k;
                if j == first {
                    break;
                }
                k -= 1;
                if !c(&t, &v[k]) {
                    break;
                }
            }
            v[j] = t;
            count += 1;
            if count == 8 {
                return i + 1 == last;
            }
        }
        j = i;
        i += 1;
    }
    true
}

/// Partitions around the pivot `v[first]`, with elements equal to it on the
/// right. Returns where the pivot ends up and whether nothing had to move.
fn partition_with_equals_on_right<T: Copy>(
    v: &mut [T],
    mut first: usize,
    mut last: usize,
    c: &impl Fn(&T, &T) -> bool,
) -> (usize, bool) {
    let begin = first;
    let pivot = v[first];
    loop {
        first += 1;
        if !c(&v[first], &pivot) {
            break;
        }
    }
    if begin == first - 1 {
        while first < last {
            last -= 1;
            if c(&v[last], &pivot) {
                break;
            }
        }
    } else {
        loop {
            last -= 1;
            if c(&v[last], &pivot) {
                break;
            }
        }
    }
    let already_partitioned = first >= last;
    while first < last {
        v.swap(first, last);
        loop {
            first += 1;
            if !c(&v[first], &pivot) {
                break;
            }
        }
        loop {
            last -= 1;
            if c(&v[last], &pivot) {
                break;
            }
        }
    }
    let pivot_pos = first - 1;
    if begin != pivot_pos {
        v[begin] = v[pivot_pos];
    }
    v[pivot_pos] = pivot;
    (pivot_pos, already_partitioned)
}

/// Partitions around the pivot `v[first]` with equal elements on the left,
/// for a range whose left neighbour equals the pivot. Returns the start of
/// the elements greater than the pivot.
fn partition_with_equals_on_left<T: Copy>(
    v: &mut [T],
    mut first: usize,
    mut last: usize,
    c: &impl Fn(&T, &T) -> bool,
) -> usize {
    let begin = first;
    let pivot = v[first];
    if c(&pivot, &v[last - 1]) {
        loop {
            first += 1;
            if c(&pivot, &v[first]) {
                break;
            }
        }
    } else {
        loop {
            first += 1;
            if first >= last || c(&pivot, &v[first]) {
                break;
            }
        }
    }
    if first < last {
        loop {
            last -= 1;
            if !c(&pivot, &v[last]) {
                break;
            }
        }
    }
    while first < last {
        v.swap(first, last);
        loop {
            first += 1;
            if c(&pivot, &v[first]) {
                break;
            }
        }
        loop {
            last -= 1;
            if !c(&pivot, &v[last]) {
                break;
            }
        }
    }
    let pivot_pos = first - 1;
    if begin != pivot_pos {
        v[begin] = v[pivot_pos];
    }
    v[pivot_pos] = pivot;
    first
}

fn introsort<T: Copy>(
    v: &mut [T],
    mut first: usize,
    mut last: usize,
    c: &impl Fn(&T, &T) -> bool,
    mut depth: isize,
    mut leftmost: bool,
) {
    const LIMIT: usize = 24;
    const NINTHER_THRESHOLD: usize = 128;
    loop {
        let len = last - first;
        match len {
            0 | 1 => return,
            2 => {
                if c(&v[last - 1], &v[first]) {
                    v.swap(first, last - 1);
                }
                return;
            }
            3 => return sort3(v, first, first + 1, last - 1, c),
            4 => return sort4(v, first, first + 1, first + 2, last - 1, c),
            5 => return sort5(v, [first, first + 1, first + 2, first + 3, last - 1], c),
            _ => {}
        }
        if len < LIMIT {
            if leftmost {
                insertion_sort(v, first, last, c);
            } else {
                insertion_sort_unguarded(v, first, last, c);
            }
            return;
        }
        if depth == 0 {
            heap_sort(&mut v[first..last], c);
            return;
        }
        depth -= 1;
        let half = len / 2;
        if len > NINTHER_THRESHOLD {
            sort3(v, first, first + half, last - 1, c);
            sort3(v, first + 1, first + half - 1, last - 2, c);
            sort3(v, first + 2, first + half + 1, last - 3, c);
            sort3(v, first + half - 1, first + half, first + half + 1, c);
            v.swap(first, first + half);
        } else {
            sort3(v, first + half, first, last - 1, c);
        }
        if !leftmost && !c(&v[first - 1], &v[first]) {
            first = partition_with_equals_on_left(v, first, last, c);
            continue;
        }
        let (mut i, already_partitioned) = partition_with_equals_on_right(v, first, last, c);
        if already_partitioned {
            let fs = insertion_sort_incomplete(v, first, i, c);
            if insertion_sort_incomplete(v, i + 1, last, c) {
                if fs {
                    return;
                }
                last = i;
                continue;
            } else if fs {
                i += 1;
                first = i;
                continue;
            }
        }
        introsort(v, first, i, c, depth, leftmost);
        leftmost = false;
        first = i + 1;
    }
}

fn sift_down<T: Copy>(h: &mut [T], len: usize, mut start: usize, c: &impl Fn(&T, &T) -> bool) {
    let mut child = start;
    if len < 2 || (len - 2) / 2 < child {
        return;
    }
    child = 2 * child + 1;
    if child + 1 < len && c(&h[child], &h[child + 1]) {
        child += 1;
    }
    if c(&h[child], &h[start]) {
        return;
    }
    let top = h[start];
    loop {
        h[start] = h[child];
        start = child;
        if (len - 2) / 2 < child {
            break;
        }
        child = 2 * child + 1;
        if child + 1 < len && c(&h[child], &h[child + 1]) {
            child += 1;
        }
        if c(&h[child], &top) {
            break;
        }
    }
    h[start] = top;
}

fn floyd_sift_down<T: Copy>(h: &mut [T], len: usize, c: &impl Fn(&T, &T) -> bool) -> usize {
    let mut hole = 0;
    let mut child = 0;
    loop {
        child = 2 * child + 1;
        if child + 1 < len && c(&h[child], &h[child + 1]) {
            child += 1;
        }
        h[hole] = h[child];
        hole = child;
        if child > (len - 2) / 2 {
            return hole;
        }
    }
}

fn sift_up<T: Copy>(h: &mut [T], mut last: usize, mut len: usize, c: &impl Fn(&T, &T) -> bool) {
    if len > 1 {
        len = (len - 2) / 2;
        let mut ptr = len;
        last -= 1;
        if c(&h[ptr], &h[last]) {
            let t = h[last];
            loop {
                h[last] = h[ptr];
                last = ptr;
                if len == 0 {
                    break;
                }
                len = (len - 1) / 2;
                ptr = len;
                if !c(&h[ptr], &t) {
                    break;
                }
            }
            h[last] = t;
        }
    }
}

fn pop_heap<T: Copy>(h: &mut [T], len: usize, c: &impl Fn(&T, &T) -> bool) {
    if len > 1 {
        let top = h[0];
        let mut hole = floyd_sift_down(h, len, c);
        let last = len - 1;
        if hole == last {
            h[hole] = top;
        } else {
            h[hole] = h[last];
            hole += 1;
            h[last] = top;
            sift_up(h, hole, hole, c);
        }
    }
}

fn make_heap<T: Copy>(h: &mut [T], c: &impl Fn(&T, &T) -> bool) {
    let n = h.len();
    if n > 1 {
        for start in (0..=(n - 2) / 2).rev() {
            sift_down(h, n, start, c);
        }
    }
}

fn sort_heap<T: Copy>(h: &mut [T], c: &impl Fn(&T, &T) -> bool) {
    let mut n = h.len();
    while n > 1 {
        pop_heap(h, n, c);
        n -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rand::Rand;
    use crate::test_util::Fnv;

    // From rust/liero-game/reference/reference_values.cpp.
    const EXPECTED_SORT: u64 = 0x080bcfd853b625b4;
    const EXPECTED_HEAP_SORT: u64 = 0x8434e9eaacf0bffa;

    #[derive(Clone, Copy)]
    struct Item {
        key: i32,
        id: i32,
    }

    /// The same arrays as the C++: mostly up to 80 elements, sometimes up to
    /// 2,000 to reach the ninther and the heap sort, with few distinct keys,
    /// laid out at random, ascending, descending, as an organ pipe, nearly
    /// descending or all equal.
    fn arrays(seed: u32, mut f: impl FnMut(&mut Vec<Item>)) {
        let mut r = Rand::new(seed);
        for t in 0..20000u32 {
            let n = if t % 50 == 0 { r.below(2000) } else { r.below(81) };
            let keys = 1 + r.below(if t % 3 == 0 { 3 } else { 40 });
            let pattern = r.below(6);
            let d = n.max(1);
            let mut v: Vec<Item> = (0..n)
                .map(|i| {
                    let key = match pattern {
                        1 => i * keys / d,
                        2 | 4 => (n - i) * keys / d,
                        3 => i.min(n - i) * keys * 2 / d,
                        5 => 0,
                        _ => r.below(keys),
                    };
                    Item {
                        key: key as i32,
                        id: i as i32,
                    }
                })
                .collect();
            if pattern == 4 && n > 1 {
                for _ in 0..3 {
                    let a = r.below(n) as usize;
                    let b = r.below(n) as usize;
                    v.swap(a, b);
                }
            }
            f(&mut v);
        }
    }

    type SortFn = fn(&mut [Item], &dyn Fn(&Item, &Item) -> bool);

    fn order_hash(seed: u32, sort_fn: SortFn) -> u64 {
        let mut h = Fnv::new();
        arrays(seed, |v| {
            sort_fn(v, &|a: &Item, b: &Item| a.key > b.key);
            h.add(v.len() as u32);
            for e in v.iter() {
                h.add(e.id as u32);
            }
        });
        h.finish()
    }

    #[test]
    fn sorts_like_libcxx() {
        assert_eq!(order_hash(4242, |v, c| sort(v, &|a, b| c(a, b))), EXPECTED_SORT);
    }

    #[test]
    fn heap_sorts_like_libcxx() {
        assert_eq!(
            order_hash(4243, |v, c| heap_sort(v, &|a, b| c(a, b))),
            EXPECTED_HEAP_SORT
        );
    }

    #[test]
    fn sorts() {
        arrays(99, |v| {
            sort(v, &|a, b| a.key > b.key);
            assert!(v.windows(2).all(|w| w[0].key >= w[1].key));
            let mut ids: Vec<i32> = v.iter().map(|e| e.id).collect();
            ids.sort();
            assert!(ids.iter().enumerate().all(|(i, &id)| id == i as i32));
        });
    }
}
