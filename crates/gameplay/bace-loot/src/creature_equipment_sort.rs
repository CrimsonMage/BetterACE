//! .NET List.Sort introsort ordering, including its unstable equal-key swaps.
//! Required because source reverses each sorted list before selecting equipment.
use std::cmp::Ordering;
pub(crate) fn sort(values: &mut [usize], compare: impl Fn(usize, usize) -> Ordering + Copy) {
    if values.len() > 1 {
        intro(
            values,
            2 * (usize::BITS - values.len().leading_zeros()) as usize,
            compare,
        );
    }
}
fn swap_if(a: &mut [usize], x: usize, y: usize, c: impl Fn(usize, usize) -> Ordering) {
    if c(a[x], a[y]).is_gt() {
        a.swap(x, y);
    }
}
fn intro(a: &mut [usize], mut depth: usize, c: impl Fn(usize, usize) -> Ordering + Copy) {
    let mut size = a.len();
    while size > 1 {
        if size <= 16 {
            match size {
                2 => swap_if(a, 0, 1, c),
                3 => {
                    swap_if(a, 0, 1, c);
                    swap_if(a, 0, 2, c);
                    swap_if(a, 1, 2, c);
                }
                _ => {
                    for i in 1..size {
                        let t = a[i];
                        let mut j = i;
                        while j > 0 && c(t, a[j - 1]).is_lt() {
                            a[j] = a[j - 1];
                            j -= 1;
                        }
                        a[j] = t;
                    }
                }
            }
            return;
        }
        if depth == 0 {
            heap(&mut a[..size], c);
            return;
        }
        depth -= 1;
        let high = size - 1;
        let middle = high / 2;
        swap_if(a, 0, middle, c);
        swap_if(a, 0, high, c);
        swap_if(a, middle, high, c);
        let pivot = a[middle];
        a.swap(middle, high - 1);
        let mut left = 0;
        let mut right = high - 1;
        loop {
            left += 1;
            while c(a[left], pivot).is_lt() {
                left += 1;
            }
            right -= 1;
            while c(pivot, a[right]).is_lt() {
                right -= 1;
            }
            if left >= right {
                break;
            }
            a.swap(left, right);
        }
        a.swap(left, high - 1);
        intro(&mut a[left + 1..size], depth, c);
        size = left;
    }
}
fn heap(a: &mut [usize], c: impl Fn(usize, usize) -> Ordering + Copy) {
    fn down(a: &mut [usize], mut i: usize, n: usize, c: impl Fn(usize, usize) -> Ordering) {
        let d = a[i - 1];
        while i <= n / 2 {
            let mut child = 2 * i;
            if child < n && c(a[child - 1], a[child]).is_lt() {
                child += 1;
            }
            if !c(d, a[child - 1]).is_lt() {
                break;
            }
            a[i - 1] = a[child - 1];
            i = child;
        }
        a[i - 1] = d;
    }
    let n = a.len();
    for i in (1..=n / 2).rev() {
        down(a, i, n, c);
    }
    for i in (2..=n).rev() {
        a.swap(0, i - 1);
        down(a, 1, i - 1, c);
    }
}
