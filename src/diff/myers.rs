use crate::range::{DiffRange, Range};
use std::ops::{Index, IndexMut};

// A D-path is a path which starts at (0,0) that has exactly D non-diagonal edges. All D-paths
// consist of a (D - 1)-path followed by a non-diagonal edge and then a possibly empty sequence of
// diagonal edges called a snake.

/// `V` contains the endpoints of the furthest reaching `D-paths`. For each recorded endpoint
/// `(x,y)` in diagonal `k`, we only need to retain `x` because `y` can be computed from `x - k`.
/// In other words, `V` is an array of integers where `V[k]` contains the row index of the endpoint
/// of the furthest reaching path in diagonal `k`.
///
/// We can't use a traditional Vec to represent `V` since we use `k` as an index and it can take on
/// negative values. So instead `V` is represented as a light-weight wrapper around a Vec plus an
/// `offset` which is the maximum value `k` can take on in order to map negative `k`'s back to a
/// value >= 0.
#[derive(Debug, Clone)]
struct V {
    offset: isize,
    v: Vec<usize>,
}

impl V {
    fn new(max_d: usize) -> Self {
        Self {
            offset: max_d as isize,
            v: vec![0; 2 * max_d],
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

/// A `Snake` is a sequence of diagonal edges in the edit graph. It is possible for a snake to have
/// a length of zero, meaning the start and end points are the same.
#[derive(Debug)]
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

fn max_d(len1: usize, len2: usize) -> usize {
    // The middle-snake search iterates `d` from 0 to ceil((N+M)/2)
    // inclusive. `(len1 + len2 + 1) / 2` is that ceiling, and the trailing
    // `+ 1` converts it to an exclusive upper bound for `0..d_max`.
    (len1 + len2 + 1) / 2 + 1
}

/// Tunables for the Myers middle-snake heuristic bailouts.
struct HeuristicsConfig {
    /// Maximum `d` (edit cost) the middle-snake search is allowed to
    /// explore before bailing out with a heuristic split.
    #[allow(unused)]
    max_cost: usize,
}

impl HeuristicsConfig {
    /// Sets the floor for the max_cost heuristic so that small inputs still get an optimal diff.
    const MAX_COST_MINIMUM: usize = 256;

    fn new(n: usize, m: usize) -> Self {
        // Calculate a rough estimate of sqrt(n+m) to determine a max_cost
        let nm = n + m;
        let bits = usize::BITS - nm.leading_zeros();
        let max_cost = (1usize << ((bits + 1) / 2)).max(Self::MAX_COST_MINIMUM);
        // let max_cost = ((nm as f64).sqrt() as usize).max(Self::MAX_COST_MINIMUM);
        Self { max_cost }
    }
}

/// The result of searching for a middle snake.
///
/// `Optimal` is returned when forward and backward paths overlap within the
/// edit budget; the contained snake is the real middle snake. `Heuristic`
/// is returned when `max_cost` is exceeded first; the contained snake is a
/// synthesized zero-length split point at the furthest-reaching endpoint
/// seen during the search. The resulting diff is correct but not guaranteed
/// to be minimal.
struct SplitResult {
    snake: Snake,
    need_minimal_forward: bool,
    need_minimal_backward: bool,
}

// The divide part of a divide-and-conquer strategy. A D-path has D+1 snakes some of which may
// be empty. The divide step requires finding the ceil(D/2) + 1 or middle snake of an optimal
// D-path. The idea for doing so is to simultaneously run the basic algorithm in both the
// forward and reverse directions until furthest reaching forward and reverse paths starting at
// opposing corners 'overlap'.
//
// When `max_cost` is exceeded before an overlap is found, the search bails
// out and returns a `SplitResult::Heuristic` anchored at whichever search
// direction made more progress (measured by `x + y` in its own coordinate
// system). This produces a non-minimal but correct diff in bounded time.
fn find_middle_snake<T: PartialEq>(
    old: Range<'_, [T]>,
    new: Range<'_, [T]>,
    vf: &mut V,
    vb: &mut V,
    heuristic: &HeuristicsConfig,
    need_minimal: bool,
) -> SplitResult {
    let n = old.len();
    let m = new.len();

    // By Lemma 1 in the paper, the optimal edit script length is odd or even as `delta` is odd
    // or even.
    let delta = n as isize - m as isize;
    let odd = delta & 1 == 1;

    // The initial point at (0, -1)
    vf[1] = 0;
    // The initial point at (N, M+1)
    vb[1] = 0;

    // We only need to explore ceil(D/2) + 1
    let d_max = max_d(n, m);
    assert!(vf.len() >= d_max);
    assert!(vb.len() >= d_max);

    // Furthest-reaching snake seen on each side. Every forward or backward
    // extension produces a snake `(x0, y0) -> (x, y)` of zero or more matching
    // elements; we remember the one whose endpoint has made the most progress
    // along its search axis, scored by `x + y` in that side's own coordinate
    // frame.
    //
    // `best_fwd` is stored in actual grid coords. `best_bwd` stays in
    // backward-stored coords because converting on each update is wasteful;
    // the heuristic bail below maps it to actual coords once.
    let mut best_fwd_snake = Snake {
        x_start: 0,
        y_start: 0,
        x_end: 0,
        y_end: 0,
    };
    let mut best_fwd_score: usize = 0;
    let mut best_bwd_snake = Snake {
        x_start: 0,
        y_start: 0,
        x_end: 0,
        y_end: 0,
    };
    let mut best_bwd_score: usize = 0;

    for d in 0..d_max as isize {
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
            if let (Some(s1), Some(s2)) = (old.get(x..), new.get(y..)) {
                let advance = s1.common_prefix_len(s2);
                x += advance;
                y += advance;
            }

            // This is the new best x value
            vf[k] = x;

            // On asymmetric inputs a `vf[k - 1] + 1` step can push `x`
            // past `n`, or a large `|k|` can push `y = x - k` outside
            // `[0, m]`. Such probes are still stored so the `(k-1)+1`
            // chain reads a consistent value next iteration, but they
            // must not contribute to `best_fwd` or fire the overlap
            // check — a snake built from their coords would overflow
            // `n - x` arithmetic in `conquer`'s `split_at` call.
            let in_box = x <= n && y <= m;
            if in_box && x + y > best_fwd_score {
                best_fwd_score = x + y;
                best_fwd_snake = Snake {
                    x_start: x0,
                    y_start: y0,
                    x_end: x,
                    y_end: y,
                };
            }
            // Only check for connections from the forward search when N - M is odd
            // and when there is a reciprocal k line coming from the other direction.
            if odd && (k - delta).abs() <= (d - 1) {
                // Forward x-coordinate plus the reciprocal backward distance
                // from (N, M) meets or exceeds `n` exactly when the two paths
                // have crossed on the forward axis.
                if vf[k] + vb[-(k - delta)] >= n {
                    return SplitResult {
                        snake: Snake {
                            x_start: x0,
                            y_start: y0,
                            x_end: x,
                            y_end: y,
                        },
                        need_minimal_forward: true,
                        need_minimal_backward: true,
                    };
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
            if x < n && y < m {
                let advance = old.slice(..n - x).common_suffix_len(new.slice(..m - y));
                x += advance;
                y += advance;
            }

            // This is the new best x value
            vb[k] = x;

            // Same overshoot concern as the forward side: the reversed-
            // coord `vb[k - 1] + 1` step is not bounds-checked against
            // the top edge of the box, so `x` can exceed `n` and `y`
            // can exceed `m`. Such probes are kept in `vb` for the
            // next iteration to read, but they're excluded from the
            // best-so-far tracking and the overlap check — a snake
            // built from their coords would underflow the `n - x` /
            // `m - y` conversion.
            let in_box = x <= n && y <= m;
            if in_box && x + y > best_bwd_score {
                best_bwd_score = x + y;
                best_bwd_snake = Snake {
                    x_start: x,
                    y_start: y,
                    x_end: x0,
                    y_end: y0,
                };
            }

            if !odd && (k - delta).abs() <= d {
                // Backward x-distance plus the reciprocal forward x-coordinate
                // meets or exceeds `n` exactly when the two paths have crossed
                // on the forward axis.
                if vb[k] + vf[-(k - delta)] >= n {
                    return SplitResult {
                        snake: Snake {
                            x_start: n - x,
                            y_start: m - y,
                            x_end: n - x0,
                            y_end: m - y0,
                        },
                        need_minimal_forward: true,
                        need_minimal_backward: true,
                    };
                }
            }
        }

        // Heuristic bail. Once `d` reaches `heuristic.max_cost` we stop
        // searching for the optimal middle snake and return whichever side
        // has made more progress as the split point. Returning the full
        // snake (not just its endpoint) lets `conquer` emit the confirmed
        // matching content directly, instead of rediscovering it via
        // prefix/suffix scans.
        //
        // We require `d >= 1` to guarantee the split is non-trivial —
        // bailing at `d == 0` with both sides at zero progress would split
        // at (0, 0) and recurse on the full problem, causing infinite
        // recursion.
        if !need_minimal && d >= 1 && (d as usize) >= heuristic.max_cost {
            let res = if best_fwd_score >= best_bwd_score {
                SplitResult {
                    snake: best_fwd_snake,
                    need_minimal_forward: true,
                    need_minimal_backward: false,
                }
            } else {
                // Convert stored backward coords to actual grid coords.
                // The backward snake runs from higher stored values toward
                // lower, so the actual-coord ordering flips: stored
                // `x_start` is the actual end, and stored `x_end` is the
                // actual start.
                SplitResult {
                    snake: Snake {
                        x_start: n - best_bwd_snake.x_start,
                        y_start: m - best_bwd_snake.y_start,
                        x_end: n - best_bwd_snake.x_end,
                        y_end: m - best_bwd_snake.y_end,
                    },
                    need_minimal_forward: true,
                    need_minimal_backward: false,
                }
            };
            return res;
        }
    }

    // With `need_minimal` or `heuristic.max_cost >= d_max` the bail never
    // fires, and Lemma 1 still guarantees a middle snake is found — so
    // this is only reachable if the input violates the algorithm's
    // preconditions.
    unreachable!("unable to find a middle snake");
}

fn conquer<'a, 'b, T: PartialEq>(
    mut old: Range<'a, [T]>,
    mut new: Range<'b, [T]>,
    vf: &mut V,
    vb: &mut V,
    heuristics: &HeuristicsConfig,
    need_minimal: bool,
    solution: &mut Vec<DiffRange<'a, 'b, [T]>>,
) {
    // Check for common prefix
    let common_prefix_len = old.common_prefix_len(new);
    if common_prefix_len > 0 {
        let common_prefix = DiffRange::Equal(
            old.slice(..common_prefix_len),
            new.slice(..common_prefix_len),
        );
        solution.push(common_prefix);
    }

    old = old.slice(common_prefix_len..old.len());
    new = new.slice(common_prefix_len..new.len());

    // Check for common suffix
    let common_suffix_len = old.common_suffix_len(new);
    let common_suffix = DiffRange::Equal(
        old.slice(old.len() - common_suffix_len..),
        new.slice(new.len() - common_suffix_len..),
    );
    old = old.slice(..old.len() - common_suffix_len);
    new = new.slice(..new.len() - common_suffix_len);

    if old.is_empty() && new.is_empty() {
        // Do nothing
    } else if old.is_empty() {
        // Inserts
        solution.push(DiffRange::Insert(new));
    } else if new.is_empty() {
        // Deletes
        solution.push(DiffRange::Delete(old));
    } else {
        // Divide & Conquer. The optimal-vs-heuristic distinction doesn't
        // matter here — either way we split at `(snake.x_start, snake.y_start)`
        // and recurse on the two halves.
        let SplitResult {
            snake,
            need_minimal_forward,
            need_minimal_backward,
        } = find_middle_snake(old, new, vf, vb, heuristics, need_minimal);

        let (old_a, old_b) = old.split_at(snake.x_start);
        let (new_a, new_b) = new.split_at(snake.y_start);

        conquer(
            old_a,
            new_a,
            vf,
            vb,
            heuristics,
            need_minimal_forward,
            solution,
        );
        conquer(
            old_b,
            new_b,
            vf,
            vb,
            heuristics,
            need_minimal_backward,
            solution,
        );
    }

    if common_suffix_len > 0 {
        solution.push(common_suffix);
    }
}

pub fn diff<'a, 'b, T: PartialEq>(
    old: &'a [T],
    new: &'b [T],
    need_minimal: bool,
) -> Vec<DiffRange<'a, 'b, [T]>> {
    let old_recs = Range::new(old, ..);
    let new_recs = Range::new(new, ..);

    let mut solution = Vec::new();

    // The arrays that hold the 'best possible x values' in search from:
    // `vf`: top left to bottom right
    // `vb`: bottom right to top left
    let max_d = max_d(old.len(), new.len());
    let mut vf = V::new(max_d);
    let mut vb = V::new(max_d);

    let heuristics = HeuristicsConfig::new(old.len(), new.len());

    conquer(
        old_recs,
        new_recs,
        &mut vf,
        &mut vb,
        &heuristics,
        need_minimal,
        &mut solution,
    );

    solution
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_middle_snake() {
        let a = Range::new(&b"ABCABBA"[..], ..);
        let b = Range::new(&b"CBABAC"[..], ..);
        let max_d = max_d(a.len(), b.len());
        let mut vf = V::new(max_d);
        let mut vb = V::new(max_d);
        let heuristics = HeuristicsConfig::new(a.len(), b.len());
        find_middle_snake(a, b, &mut vf, &mut vb, &heuristics, true);
    }
}
