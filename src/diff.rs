#[derive(Debug)]
struct Line<'a> {
    number: usize,
    text: &'a [u8],
}

pub fn diff(a: &[u8], b: &[u8]) {
    let a = lines(a);
    let b = lines(b);

    Myers::gen_diff(Myers::backtrace(Myers::diff(&a, &b), &a, &b), &a, &b);
}

fn lines(a: &[u8]) -> Vec<Line> {
    a.split(|c| *c == b'\n')
        .enumerate()
        .map(|(ln, text)| Line {
            number: ln + 1,
            text,
        })
        .collect()
}

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

#[derive(Debug)]
struct Record<'a> {
    inner: &'a u8,
    changed: bool,
}

impl PartialEq for Record<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

struct Myers;

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

    fn conquer(mut old: &mut [Record], mut new: &mut [Record], vf: &mut V, vb: &mut V) {
        let mut start_old = 0;
        let mut start_new = 0;
        let mut end_old = old.len();
        let mut end_new = new.len();

        while start_old < end_old
            && start_new < end_new
            && old[start_old].inner == new[start_new].inner
        {
            start_old += 1;
            start_new += 1;
        }
        while start_old < end_old
            && start_new < end_new
            && old[end_old - 1].inner == new[end_new - 1].inner
        {
            end_old -= 1;
            end_new -= 1;
        }

        old = &mut old[start_old..end_old];
        new = &mut new[start_new..end_new];

        if old.is_empty() {
            for mut record in new {
                record.changed = true;
            }
        } else if new.is_empty() {
            for mut record in old {
                record.changed = true;
            }
        } else {
            // Divide & Conquer
            let (shortest_edit_script_len, snake) = Self::find_middle_snake(&old, &new, vf, vb);

            let (old_a, old_b) = old.split_at_mut(snake.x_start);
            let (new_a, new_b) = new.split_at_mut(snake.y_start);

            Self::conquer(old_a, new_a, vf, vb);
            Self::conquer(old_b, new_b, vf, vb);
        }
    }

    fn do_diff<'a, 'b>(old: &'a [u8], new: &'b [u8]) -> (Vec<Record<'a>>, Vec<Record<'b>>) {
        let mut old: Vec<Record> = old
            .into_iter()
            .map(|a| Record {
                inner: a,
                changed: false,
            })
            .collect();
        let mut new: Vec<Record> = new
            .into_iter()
            .map(|a| Record {
                inner: a,
                changed: false,
            })
            .collect();

        let max = old.len() + new.len();
        // The array that holds the 'best possible x values' in search from top left to bottom right.
        let mut vf = V::new(max + 3, old.len());
        // The array that holds the 'best possible x values' in search from bottom right to top left.
        let mut vb = V::new(max + 3, old.len());

        Self::conquer(&mut old, &mut new, &mut vf, &mut vb);

        Self::gen_diff1(&old, &new);
        (old, new)
    }

    fn gen_diff1(old: &[Record<'_>], new: &[Record<'_>]) {
        let mut num1 = 0;
        let mut num2 = 0;

        while num1 < old.len() || num2 < new.len() {
            if num1 < old.len() && old[num1].changed {
                println!(
                    "\x1b[0;31m- {: <4}      {}\x1b[0m",
                    num1 + 1,
                    *old[num1].inner as char,
                );
                num1 += 1;
            } else if num2 < new.len() && new[num2].changed {
                println!(
                    "\x1b[0;32m+      {: <4} {}\x1b[0m",
                    num2 + 1,
                    *new[num2].inner as char,
                );
                num2 += 1;
            } else {
                println!(
                    "  {: <4} {: <4} {}",
                    num1 + 1,
                    num2 + 1,
                    *old[num1].inner as char
                );
                num1 += 1;
                num2 += 1;
            }
        }
    }

    fn diff(a: &Vec<Line>, b: &Vec<Line>) -> Vec<V> {
        let n = a.len();
        let m = b.len();
        let max = n + m;
        let mut v = V::new(2 * max + 1, max);
        let mut trace = Vec::new();

        for d in 0..max as isize {
            trace.push(v.clone());

            let mut k = -d;
            while k <= d {
                let mut x = if k == -d || (k != d && v[k - 1] < v[k + 1]) {
                    v[k + 1]
                } else {
                    v[k - 1] + 1
                };
                let mut y = (x as isize - k) as usize;

                while x < n && y < m && a[x].text == b[y].text {
                    x += 1;
                    y += 1;
                }

                v[k] = x;

                if x >= n && y >= m {
                    return trace;
                }

                k += 2;
            }
        }

        trace
    }

    fn backtrace(trace: Vec<V>, a: &Vec<Line>, b: &Vec<Line>) -> Vec<(usize, usize, usize, usize)> {
        let mut x = a.len();
        let mut y = b.len();
        let mut path = Vec::new();

        for (d, v) in trace.iter().enumerate().rev() {
            let d = d as isize;
            let k = x as isize - y as isize;

            let prev_k = if k == -d || (k != d && v[k - 1] < v[k + 1]) {
                k + 1
            } else {
                k - 1
            };
            let prev_x = v[prev_k];
            let prev_y = (prev_x as isize - prev_k) as usize;

            while x > prev_x && y > prev_y {
                path.push((x - 1, y - 1, x, y));
                x -= 1;
                y -= 1;
            }

            if d > 0 {
                path.push((prev_x, prev_y, x, y));
            }

            x = prev_x;
            y = prev_y;
        }

        path
    }

    fn gen_diff(path: Vec<(usize, usize, usize, usize)>, a: &Vec<Line>, b: &Vec<Line>) {
        let mut diff = Vec::new();

        for &(prev_x, prev_y, x, y) in path.iter().rev() {
            if x == prev_x {
                let b_line = &b[prev_y];
                diff.push(Edit::Insertion(b_line));
            } else if y == prev_y {
                let a_line = &a[prev_x];
                diff.push(Edit::Deletion(a_line));
            } else {
                let a_line = &a[prev_x];
                let b_line = &b[prev_y];
                diff.push(Edit::Equal(a_line, b_line));
            }
        }

        // Print Diff
        for edit in diff {
            match edit {
                Edit::Insertion(line) => println!(
                    "\x1b[0;32m+      {: <4} {}\x1b[0m",
                    line.number,
                    String::from_utf8_lossy(line.text)
                ),
                Edit::Deletion(line) => println!(
                    "\x1b[0;31m- {: <4}      {}\x1b[0m",
                    line.number,
                    String::from_utf8_lossy(line.text)
                ),
                Edit::Equal(a, b) => println!(
                    "  {: <4} {: <4} {}",
                    a.number,
                    b.number,
                    String::from_utf8_lossy(b.text)
                ),
            }
        }
    }
}

enum Edit<'b, 'a: 'b> {
    Insertion(&'b Line<'a>),
    Deletion(&'b Line<'a>),
    Equal(&'b Line<'a>, &'b Line<'a>),
}

#[cfg(test)]
mod tests {
    use crate::diff::lines;
    use crate::diff::Myers;
    use crate::diff::V;

    #[test]
    fn diff_test1() {
        let a = b"ABCABBA";
        let b = b"CBABAC";
        let max = a.len() + b.len();
        // The array that holds the 'best possible x values' in search from top left to bottom right.
        let mut vf = V::new(max + 3, a.len());
        // The array that holds the 'best possible x values' in search from bottom right to top left.
        let mut vb = V::new(max + 3, a.len());
        Myers::find_middle_snake(&a[..], &b[..], &mut vf, &mut vb);
    }

    #[test]
    fn diff_test2() {
        let a = "ABCABBA";
        let b = "CBABAC";
        let (a, b) = Myers::do_diff(a.as_bytes(), b.as_bytes());
    }

    #[test]
    fn diff_test3() {
        let a = "abgdef";
        let b = "gh";
        let (a, b) = Myers::do_diff(a.as_bytes(), b.as_bytes());
    }
}
