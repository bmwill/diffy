use std::ops::{Index, IndexMut};

// A D-path is a path which starts at (0,0) that has exactly D non-diagonal edges. All D-paths
// consist of a (D - 1)-path followed by a non-diagonal edge and then a possibly empty sequence of
// diagonal edges called a snake.

#[derive(Debug, Clone)]
/// `V` contains the endpoints of the furthest reaching `D-paths`. For each recorded endpoint
/// `(x,y)` in diagonal `k`, we only need to retain `x` because `y` can be computed from `x - k`.
/// In other words, `V` is an array of integers where `V[k]` contains the row index of the endpoint
/// of the furthest reaching path in diagonal `k`.
///
/// We can't use a traditional Vec to represent `V` since we use `k` as an index and it can take on
/// negative values. So instead `V` is represented as a light-weight wrapper around a Vec plus an
/// `offset` which is the maximum value `k` can take on in order to map negative `k`'s back to a
/// value >= 0.
struct V {
    offset: isize,
    v: Vec<usize>,
}

impl V {
    fn new(size: usize, offset: usize) -> Self {
        Self {
            offset: offset as isize,
            v: vec![0; size],
        }
    }

    fn len(&self) -> usize {
        self.v.len()
    }
}

impl Index<isize> for V {
    type Output = usize;

    fn index(&self, index: isize) -> &Self::Output {
        &self.v[(index + self.offset) as usize]
    }
}

impl IndexMut<isize> for V {
    fn index_mut(&mut self, index: isize) -> &mut Self::Output {
        &mut self.v[(index + self.offset) as usize]
    }
}

#[derive(Debug)]
/// A `Snake` is a sequence of diagonal edges in the edit graph. It is possible for a snake to have
/// a length of zero, meaning the start and end points are the same.
struct Snake {
    x_start: usize,
    y_start: usize,
    x_end: usize,
    y_end: usize,
}

impl ::std::fmt::Display for Snake {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(
            f,
            "({}, {}) -> ({}, {})",
            self.x_start, self.y_start, self.x_end, self.y_end
        )
    }
}

struct Records<'a, T> {
    inner: &'a [T],
    changed: &'a mut [bool],
}

impl<'a, T> Records<'a, T> {
    fn new(inner: &'a [T], changed: &'a mut [bool]) -> Self {
        debug_assert!(inner.len() == changed.len());
        Records { inner, changed }
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn slice(&mut self, begin: usize, end: usize) -> Records<'_, T> {
        Records::new(&self.inner[begin..end], &mut self.changed[begin..end])
    }

    fn split_at_mut(&mut self, mid: usize) -> (Records<'_, T>, Records<'_, T>) {
        let (left_inner, right_inner) = self.inner.split_at(mid);
        let (left_changed, right_changed) = self.changed.split_at_mut(mid);

        (
            Records::new(left_inner, left_changed),
            Records::new(right_inner, right_changed),
        )
    }
}

pub struct Myers;

impl Myers {
    // The divide part of a divide-and-conquer strategy. A D-path has D+1 snakes some of which may
    // be empty. The divide step requires finding the ceil(D/2) + 1 or middle snake of an optimal
    // D-path. The idea for doing so is to simultaneously run the basic algorithm in both the
    // forward and reverse directions until furthest reaching forward and reverse paths starting at
    // opposing corners 'overlap'.
    fn find_middle_snake<T: PartialEq>(
        old: &[T],
        new: &[T],
        vf: &mut V,
        vb: &mut V,
    ) -> (isize, Snake) {
        // In the original paper `n = old.len()` and `m = new.len()`

        // Sum of the length of the sequences being compared
        let max = old.len() + new.len();

        // By Lemma 1 in the paper, the optimal edit script length is odd or even as `delta` is odd
        // or even.
        let delta = old.len() as isize - new.len() as isize;
        let odd = delta & 1 == 1;

        debug_assert!(vf.len() >= max + 3);
        debug_assert!(vb.len() >= max + 3);

        // The initial point at (0, -1)
        vf[1] = 0;
        // The initial point at (N, M+1)
        vb[1] = 0;

        // We only need to explore ceil(D/2) + 1
        let d_max = ((max + 1) / 2 + 1) as isize;
        for d in 0..d_max {
            // Forward path
            for k in (-d..=d).rev().step_by(2) {
                let mut x = if k == -d || (k != d && vf[k - 1] < vf[k + 1]) {
                    vf[k + 1]
                } else {
                    vf[k - 1] + 1
                };
                let mut y = (x as isize - k) as usize;

                // The coordinate of the start of a snake
                let (x0, y0) = (x, y);
                //  While these sequences are identical, keep moving through the graph with no cost
                while x < old.len() && y < new.len() && old[x] == new[y] {
                    x += 1;
                    y += 1;
                }

                // This is the new best x value
                vf[k] = x;
                // Only check for connections from the forward search when N - M is odd
                // and when there is a reciprocal k line coming from the other direction.
                if odd && (k - delta).abs() <= (d - 1) {
                    // TODO optimize this so we don't have to compare against old.len()
                    if vf[k] + vb[-(k - delta)] >= old.len() {
                        // Return the snake
                        let snake = Snake {
                            x_start: x0,
                            y_start: y0,
                            x_end: x,
                            y_end: y,
                        };
                        // Edit distance to this snake is `2 * d - 1`
                        return (2 * d - 1, snake);
                    }
                }
            }

            // Backward path
            for k in (-d..=d).rev().step_by(2) {
                let mut x = if k == -d || (k != d && vb[k - 1] < vb[k + 1]) {
                    vb[k + 1]
                } else {
                    vb[k - 1] + 1
                };
                let mut y = (x as isize - k) as usize;

                // The coordinate of the start of a snake
                let (x0, y0) = (x, y);
                //  While these sequences are identical, keep moving through the graph with no cost
                while x < old.len()
                    && y < new.len()
                    && old[old.len() - x - 1] == new[new.len() - y - 1]
                {
                    x += 1;
                    y += 1;
                }

                // This is the new best x value
                vb[k] = x;

                if !odd && (k - delta).abs() <= d {
                    // TODO optimize this so we don't have to compare against old.len()
                    if vb[k] + vf[-(k - delta)] >= old.len() {
                        // Return the snake
                        let snake = Snake {
                            x_start: old.len() - x,
                            y_start: new.len() - y,
                            x_end: old.len() - x0,
                            y_end: new.len() - y0,
                        };
                        // Edit distance to this snake is `2 * d`
                        return (2 * d, snake);
                    }
                }
            }

            // TODO: Maybe there's an opportunity to optimize and bail early?
        }

        unreachable!("unable to find a middle snake");
    }

    fn conquer<T: PartialEq>(mut old: Records<T>, mut new: Records<T>, vf: &mut V, vb: &mut V) {
        let mut start_old = 0;
        let mut start_new = 0;
        let mut end_old = old.len();
        let mut end_new = new.len();

        while start_old < end_old
            && start_new < end_new
            && old.inner[start_old] == new.inner[start_new]
        {
            start_old += 1;
            start_new += 1;
        }
        while start_old < end_old
            && start_new < end_new
            && old.inner[end_old - 1] == new.inner[end_new - 1]
        {
            end_old -= 1;
            end_new -= 1;
        }

        let mut old = old.slice(start_old, end_old);
        let mut new = new.slice(start_new, end_new);

        if old.is_empty() {
            for changed in new.changed {
                *changed = true;
            }
        } else if new.is_empty() {
            for changed in old.changed {
                *changed = true;
            }
        } else {
            // Divide & Conquer
            let (_shortest_edit_script_len, snake) =
                Self::find_middle_snake(&old.inner, &new.inner, vf, vb);

            let (old_a, old_b) = old.split_at_mut(snake.x_start);
            let (new_a, new_b) = new.split_at_mut(snake.y_start);

            Self::conquer(old_a, new_a, vf, vb);
            Self::conquer(old_b, new_b, vf, vb);
        }
    }

    pub fn diff(old: &[u8], new: &[u8]) {
        let mut old_changed = vec![false; old.len()];
        let old_recs = Records::new(old, &mut old_changed);
        let mut new_changed = vec![false; new.len()];
        let new_recs = Records::new(new, &mut new_changed);

        let max = old.len() + new.len();

        // The arrays that hold the 'best possible x values' in search from:
        // `vf`: top left to bottom right
        // `vb`: bottom right to top left
        let mut vf = V::new(max + 3, old.len());
        let mut vb = V::new(max + 3, old.len());

        Self::conquer(old_recs, new_recs, &mut vf, &mut vb);

        let old_recs = Records::new(old, &mut old_changed);
        let new_recs = Records::new(new, &mut new_changed);
        Self::render_diff(&old_recs, &new_recs);
    }

    fn render_diff(old: &Records<u8>, new: &Records<u8>) {
        let mut num1 = 0;
        let mut num2 = 0;

        while num1 < old.len() || num2 < new.len() {
            if num1 < old.len() && old.changed[num1] {
                println!(
                    "\x1b[0;31m- {: <4}      {}\x1b[0m",
                    num1 + 1,
                    old.inner[num1] as char,
                );
                num1 += 1;
            } else if num2 < new.len() && new.changed[num2] {
                println!(
                    "\x1b[0;32m+      {: <4} {}\x1b[0m",
                    num2 + 1,
                    new.inner[num2] as char,
                );
                num2 += 1;
            } else {
                println!(
                    "  {: <4} {: <4} {}",
                    num1 + 1,
                    num2 + 1,
                    old.inner[num1] as char
                );
                num1 += 1;
                num2 += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::diff::{Myers, V};

    #[test]
    fn diff_test1() {
        let a = b"ABCABBA";
        let b = b"CBABAC";
        let max = a.len() + b.len();
        let mut vf = V::new(max + 3, a.len());
        let mut vb = V::new(max + 3, a.len());
        Myers::find_middle_snake(&a[..], &b[..], &mut vf, &mut vb);
    }

    #[test]
    fn diff_test2() {
        let a = "ABCABBA";
        let b = "CBABAC";
        Myers::diff(a.as_bytes(), b.as_bytes());
    }

    #[test]
    fn diff_test3() {
        let a = "abgdef";
        let b = "gh";
        Myers::diff(a.as_bytes(), b.as_bytes());
    }
}
