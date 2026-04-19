use crate::range::{DiffRange, Range};
use std::cmp;
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
    /// Minimum edit cost before the snake-length heuristic is allowed to
    /// fire. Below this, the algorithm always runs full Myers.
    heur_min: usize,
    /// Minimum snake length (in classifier IDs / bytes) that counts as
    /// "interesting" when scanning the frontier for a heuristic split
    /// point.
    snake_cnt: usize,
    /// A candidate diagonal's progress must exceed `k_heur * d` to
    /// trigger the snake heuristic.
    k_heur: usize,
    /// Maximum `d` (edit cost) the middle-snake search is allowed to
    /// explore before bailing out with a heuristic split.
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
        Self {
            heur_min: 256,
            snake_cnt: 20,
            k_heur: 4,
            max_cost,
        }
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

    // `snake_cnt` is used both to tag long snakes during the forward /
    // backward extensions (`got_snake`) and to confirm a candidate
    // diagonal in the snake-length heuristic block below.
    let snake_cnt = if need_minimal { 0 } else { heuristic.snake_cnt };

    for d in 0..d_max as isize {
        // Reset per-d: set to true when a forward or backward extension
        // produces a snake of length >= `snake_cnt`. Only then is the
        // snake-length heuristic worth scanning for.
        let mut got_snake = false;

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
            if snake_cnt > 0 && x - x0 >= snake_cnt {
                got_snake = true;
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
            if snake_cnt > 0 && x - x0 >= snake_cnt {
                got_snake = true;
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

        // From this point on the block is heuristic; skip it when we
        // must produce a minimal diff.
        if need_minimal {
            continue;
        }

        let ec = d as usize;

        // Snake-length heuristic. If the edit cost has already passed
        // `heur_min` and at least one extension this round produced a
        // snake of length >= `snake_cnt`, scan the frontier for a
        // diagonal whose progress `(x + y) - |k - delta|` exceeds
        // `k_heur * d` and confirm a real snake of the required length
        // ends / starts at that point. Returning the full snake range
        // lets `conquer` emit it as `Equal` content directly.
        if ec > heuristic.heur_min && got_snake {
            let fmid = delta;
            let old_s = old.as_slice();
            let new_s = new.as_slice();

            // Scan forward diagonals.
            let mut best: isize = 0;
            let mut best_snake: Option<Snake> = None;
            for k in (-d..=d).rev().step_by(2) {
                let x = cmp::min(vf[k], n);
                let y_signed = x as isize - k;
                if y_signed < 0 || y_signed > m as isize {
                    continue;
                }
                let y = y_signed as usize;
                let dd = (k - fmid).unsigned_abs();
                let progress = (x + y).wrapping_sub(dd) as isize;

                if progress > heuristic.k_heur as isize * d
                    && progress > best
                    && heuristic.snake_cnt <= x
                    && x < n
                    && heuristic.snake_cnt <= y
                    && y < m
                {
                    // Confirm a real snake of the required length ends
                    // at `(x, y)` by walking backward.
                    let confirmed =
                        (1..=heuristic.snake_cnt).all(|i| old_s[x - i] == new_s[y - i]);
                    if confirmed {
                        best = progress;
                        best_snake = Some(Snake {
                            x_start: x - heuristic.snake_cnt,
                            y_start: y - heuristic.snake_cnt,
                            x_end: x,
                            y_end: y,
                        });
                    }
                }
            }
            if let Some(snake) = best_snake {
                // The forward search explored the "lo" half, so that
                // side has a known cost bound of `d` and must be
                // finished minimally. The "hi" half is a fresh sub-
                // problem and may use heuristics again.
                return SplitResult {
                    snake,
                    need_minimal_forward: true,
                    need_minimal_backward: false,
                };
            }

            // Scan backward diagonals.
            best = 0;
            best_snake = None;
            for k in (-d..=d).rev().step_by(2) {
                let bx = cmp::min(vb[k], n);
                let by_signed = bx as isize - k;
                if by_signed < 0 || by_signed > m as isize {
                    continue;
                }
                let by = by_signed as usize;
                // Convert backward coords to forward.
                let x = n - bx;
                let y = m - by;
                let dd = (k - fmid).unsigned_abs();
                let progress = (bx + by).wrapping_sub(dd) as isize;

                if progress > heuristic.k_heur as isize * d
                    && progress > best
                    && x < n.saturating_sub(heuristic.snake_cnt)
                    && y < m.saturating_sub(heuristic.snake_cnt)
                {
                    // Confirm a real snake of the required length
                    // starts at `(x, y)` by walking forward.
                    let confirmed =
                        (0..heuristic.snake_cnt).all(|i| old_s[x + i] == new_s[y + i]);
                    if confirmed {
                        best = progress;
                        best_snake = Some(Snake {
                            x_start: x,
                            y_start: y,
                            x_end: x + heuristic.snake_cnt,
                            y_end: y + heuristic.snake_cnt,
                        });
                    }
                }
            }
            if let Some(snake) = best_snake {
                // The backward search explored the "hi" half, so that
                // side has a known cost bound of `d` and must be
                // finished minimally.
                return SplitResult {
                    snake,
                    need_minimal_forward: false,
                    need_minimal_backward: true,
                };
            }
        }

        // Max-cost bail. Once `d` reaches `heuristic.max_cost` we stop
        // searching for the optimal middle snake and return whichever
        // side has made more progress as the split point.
        //
        // We require `d >= 1` to guarantee the split is non-trivial —
        // bailing at `d == 0` with both sides at zero progress would
        // split at (0, 0) and recurse on the full problem, causing
        // infinite recursion.
        if d >= 1 && ec >= heuristic.max_cost {
            let res = if best_fwd_score >= best_bwd_score {
                SplitResult {
                    snake: best_fwd_snake,
                    need_minimal_forward: true,
                    need_minimal_backward: false,
                }
            } else {
                // Convert stored backward coords to actual grid coords.
                // The backward snake runs from higher stored values
                // toward lower, so the actual-coord ordering flips:
                // stored `x_start` is the actual end, and stored
                // `x_end` is the actual start.
                SplitResult {
                    snake: Snake {
                        x_start: n - best_bwd_snake.x_start,
                        y_start: m - best_bwd_snake.y_start,
                        x_end: n - best_bwd_snake.x_end,
                        y_end: m - best_bwd_snake.y_end,
                    },
                    need_minimal_forward: false,
                    need_minimal_backward: true,
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
        // Divide & Conquer. The returned snake runs from
        // `(x_start, y_start)` to `(x_end, y_end)` along the diagonal
        // and is a confirmed run of matching elements. Emit it here as
        // `Equal` content and recurse on the pre/post halves — that
        // saves the recursive calls from rediscovering the same range
        // via `common_prefix_len` / `common_suffix_len`, and for the
        // snake heuristic the range is load-bearing (the bail-point
        // split itself is otherwise arbitrary).
        let SplitResult {
            snake,
            need_minimal_forward,
            need_minimal_backward,
        } = find_middle_snake(old, new, vf, vb, heuristics, need_minimal);

        let snake_len = snake.x_end - snake.x_start;
        debug_assert_eq!(snake_len, snake.y_end - snake.y_start);

        let (old_a, old_rest) = old.split_at(snake.x_start);
        let (new_a, new_rest) = new.split_at(snake.y_start);
        let (old_mid, old_b) = old_rest.split_at(snake_len);
        let (new_mid, new_b) = new_rest.split_at(snake_len);

        conquer(
            old_a,
            new_a,
            vf,
            vb,
            heuristics,
            need_minimal_forward,
            solution,
        );
        if snake_len > 0 {
            solution.push(DiffRange::Equal(old_mid, new_mid));
        }
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
