use std::{
    marker::PhantomData,
    ops::{Deref, ShrAssign},
};

use num_traits::Zero;

use crate::op::{Loc, Timestamp};

#[derive(
    Copy,
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    derive_more::Add,
    derive_more::Deref,
    derive_more::Display,
    derive_more::From,
)]
pub struct SpaceCoord(u32);

#[derive(
    Copy,
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    derive_more::Add,
    derive_more::Deref,
    derive_more::Display,
    derive_more::From,
)]
pub struct TimeCoord(u32);

impl TimeCoord {
    pub fn from_timestamp(topo: &Topology, timestamp: Timestamp) -> Self {
        topo.time_coord(timestamp)
    }
}

pub trait Coord: From<u32> + Deref<Target = u32> {
    const MAX: u32 = u32::MAX;

    fn exp(&self, pow: u8) -> u32 {
        **self * 2u32.pow(pow as u32)
    }

    fn exp_wrapping(&self, pow: u8) -> u32 {
        (**self as u64 * 2u64.pow(pow as u32)) as u32
    }

    fn wrapping_add(self, other: u32) -> Self {
        Self::from((*self).wrapping_add(other))
    }

    fn wrapping_sub(self, other: u32) -> Self {
        Self::from((*self).wrapping_sub(other))
    }
}

impl Coord for SpaceCoord {}
impl Coord for TimeCoord {}

pub struct SpacetimeCoords {
    pub space: SpaceCoord,
    pub time: TimeCoord,
}

impl SpacetimeCoords {
    pub fn to_tuple(&self) -> (u32, u32) {
        (self.space.0, self.time.0)
    }
}

/// Any interval in space or time is represented by a node in a tree, so our
/// way of describing intervals uses tree coordinates as well:
/// The length of an interval is 2^(power), and the position of its left edge
/// is at (offset * length).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Segment<C: Coord> {
    // TODO: make `u8`?
    pub power: u32,
    pub offset: u32,
    phantom: PhantomData<C>,
}

impl<C: Coord> Segment<C> {
    pub fn new(power: u32, offset: u32) -> Self {
        Self {
            power,
            offset,
            phantom: PhantomData,
        }
    }

    pub fn length(&self) -> u64 {
        // If power is 32, this overflows a u32
        2u64.pow(self.power)
    }

    pub fn bounds(&self) -> (C, C) {
        let l = self.length();
        let o = self.offset as u64;
        (C::from((o * l) as u32), C::from((o * l + l - 1) as u32))
    }

    /// Halving an interval is equivalent to taking the child nodes of the node
    /// which represents this interval
    pub fn halve(self) -> Option<(Self, Self)> {
        if self.power == 0 {
            // Can't split a quantum value (a leaf has no children)
            None
        } else {
            let power = self.power - 1;
            Some((
                Segment::new(power, self.offset * 2),
                Segment::new(power, self.offset * 2 + 1),
            ))
        }
    }
}

pub type SpaceSegment = Segment<SpaceCoord>;
pub type TimeSegment = Segment<TimeCoord>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dimension {
    /// The smallest possible length in this dimension.
    /// Determines the interval represented by the leaf of a tree.
    quantum: u32,
    /// The size of this dimension, meaning the number of possible values
    /// that can be represented.
    ///
    /// Unused, but could be used for a more compact wire data type.
    bit_depth: u8,
}

impl Dimension {
    pub fn identity() -> Self {
        Dimension {
            quantum: 1,
            bit_depth: 32,
        }
    }

    pub const fn standard_space() -> Self {
        let quantum_power = 12;
        Dimension {
            // if a network has 1 million peers, the average spacing between them is ~4,300
            // so at a target coverage of 100, each arc will be ~430,000 in length
            // which divided by 16 is ~2700, which is about 2^15.
            // So, we'll go down to 2^12.
            // This means we only need 24 bits to represent any location.
            quantum: 2u32.pow(quantum_power),
            bit_depth: 24,
        }
    }

    pub const fn standard_time() -> Self {
        Dimension {
            // 5 minutes, in microseconds
            quantum: 1_000_000 * 60 * 5,

            // 12 quanta = 1 hour.
            // If we set the max lifetime for a network to ~100 years, which
            // is 12 * 24 * 365 * 1000 = 105,120,000 time quanta,
            // the log2 of which is 26.64,
            // then we can store any time coordinate in that range using 27 bits.
            bit_depth: 27,
        }
    }
}

/// Parameters which are constant for all time trees in a given network.
/// They determine the relationship between tree structure and absolute time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Topology {
    pub space: Dimension,
    pub time: Dimension,
    pub time_origin: Timestamp,
}

impl Topology {
    pub fn identity(time_origin: Timestamp) -> Self {
        Self {
            space: Dimension::identity(),
            time: Dimension::identity(),
            time_origin,
        }
    }

    pub fn standard(time_origin: Timestamp) -> Self {
        Self {
            space: Dimension::standard_space(),
            time: Dimension::identity(),
            time_origin,
        }
    }

    pub fn space_coord(&self, x: Loc) -> SpaceCoord {
        (x.as_u32() / self.space.quantum).into()
    }

    pub fn time_coord(&self, t: Timestamp) -> TimeCoord {
        let t = (t.as_micros() - self.time_origin.as_micros()).max(0);
        ((t / self.time.quantum as i64) as u32).into()
    }

    /// Calculate the list of exponentially shrinking time windows, as per
    /// this document: https://hackmd.io/@hololtd/r1IAIbr5Y
    pub fn telescoping_times(&self, now: Timestamp) -> Vec<TimeSegment> {
        let mut now: u32 = *self.time_coord(now) + 1;
        if now == 1 {
            return vec![];
        }
        let zs = now.leading_zeros();
        now <<= zs;
        let mut seg = TimeSegment::new(32 - zs - 1, 0);
        let mut times = vec![];
        let mask = 1u32.rotate_right(1); // 0b100000...
        for _ in 0..(32 - zs - 1) {
            seg.power -= 1;
            seg.offset *= 2;

            // remove the leading zero and shift left
            now &= !mask;
            now <<= 1;

            times.push(seg);
            seg.offset += 1;
            if now & mask > 0 {
                // if the MSB is 1, duplicate the segment
                times.push(seg);
                seg.offset += 1;
            }
        }
        // Should be all zeroes at this point
        debug_assert_eq!(now & !mask, 0);
        times
    }
}

#[derive(Copy, Clone, Debug)]
pub struct GossipParams {
    /// What +/- coordinate offset will you accept for timestamps?
    /// e.g. if the time quantum is 5 min,
    /// a time buffer of 2 will allow +/- 10 min.
    ///
    /// This, along with `max_space_power_offset`, determines what range of
    /// region resolution gets stored in the 2D Fenwick tree
    pub max_time_offset: TimeCoord,

    /// What difference in power will you accept for other agents' Arqs?
    /// e.g. if the power I use in my arq is 22, and this offset is 2,
    /// I won't talk to anyone whose arq is expressed with a power lower
    /// than 20 or greater than 24
    ///
    /// This, along with `max_time_offset`, determines what range of
    /// region resolution gets stored in the 2D Fenwick tree
    pub max_space_power_offset: u8,
}

impl GossipParams {
    pub fn zero() -> Self {
        Self {
            max_time_offset: 0.into(),
            max_space_power_offset: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_length() {
        let s = TimeSegment {
            power: 31,
            offset: 0,
            phantom: PhantomData,
        };
        assert_eq!(s.length(), 2u64.pow(31));
    }

    fn lengths(topo: &Topology, t: Timestamp) -> Vec<u32> {
        topo.telescoping_times(t)
            .into_iter()
            .map(|i| i.length() as u32)
            .collect()
    }

    #[test]
    #[rustfmt::skip]
    fn test_telescoping_times_first_16_identity_topology() {
        let topo = Topology::identity(Timestamp::from_micros(0));
        let ts = Timestamp::from_micros;

                                                                    // n++
        assert_eq!(lengths(&topo, ts(0)),  Vec::<u32>::new());      // 0001
        assert_eq!(lengths(&topo, ts(1)),  vec![1]);                // 0010
        assert_eq!(lengths(&topo, ts(2)),  vec![1, 1]);             // 0011
        assert_eq!(lengths(&topo, ts(3)),  vec![2, 1]);             // 0100
        assert_eq!(lengths(&topo, ts(4)),  vec![2, 1, 1]);          // 0101
        assert_eq!(lengths(&topo, ts(5)),  vec![2, 2, 1]);          // 0110
        assert_eq!(lengths(&topo, ts(6)),  vec![2, 2, 1, 1]);       // 0111
        assert_eq!(lengths(&topo, ts(7)),  vec![4, 2, 1]);          // 1000
        assert_eq!(lengths(&topo, ts(8)),  vec![4, 2, 1, 1]);       // 1001
        assert_eq!(lengths(&topo, ts(9)),  vec![4, 2, 2, 1]);       // 1010
        assert_eq!(lengths(&topo, ts(10)), vec![4, 2, 2, 1, 1]);    // 1011
        assert_eq!(lengths(&topo, ts(11)), vec![4, 4, 2, 1]);       // 1100
        assert_eq!(lengths(&topo, ts(12)), vec![4, 4, 2, 1, 1]);    // 1101
        assert_eq!(lengths(&topo, ts(13)), vec![4, 4, 2, 2, 1]);    // 1110
        assert_eq!(lengths(&topo, ts(14)), vec![4, 4, 2, 2, 1, 1]); // 1111
        assert_eq!(lengths(&topo, ts(15)), vec![8, 4, 2, 1]);      // 10000
    }

    #[test]
    fn test_telescoping_times_first_16_standard_topology() {
        let origin = Timestamp::now();
        let topo = Topology::standard(origin);
        let q = topo.time.quantum;
        let ts = |t| Timestamp::from_micros(origin.as_micros() + (q * t + q * 3 / 4) as i64);

        assert_eq!(lengths(&topo, ts(0)), Vec::<u32>::new());
        assert_eq!(lengths(&topo, ts(1)), vec![1]);
        assert_eq!(lengths(&topo, ts(2)), vec![1, 1]);
        assert_eq!(lengths(&topo, ts(3)), vec![2, 1]);
        assert_eq!(lengths(&topo, ts(4)), vec![2, 1, 1]);
        assert_eq!(lengths(&topo, ts(5)), vec![2, 2, 1]);
        assert_eq!(lengths(&topo, ts(6)), vec![2, 2, 1, 1]);
        assert_eq!(lengths(&topo, ts(7)), vec![4, 2, 1]);
        assert_eq!(lengths(&topo, ts(8)), vec![4, 2, 1, 1]);
        assert_eq!(lengths(&topo, ts(9)), vec![4, 2, 2, 1]);
        assert_eq!(lengths(&topo, ts(10)), vec![4, 2, 2, 1, 1]);
        assert_eq!(lengths(&topo, ts(11)), vec![4, 4, 2, 1]);
        assert_eq!(lengths(&topo, ts(12)), vec![4, 4, 2, 1, 1]);
        assert_eq!(lengths(&topo, ts(13)), vec![4, 4, 2, 2, 1]);
        assert_eq!(lengths(&topo, ts(14)), vec![4, 4, 2, 2, 1, 1]);
        assert_eq!(lengths(&topo, ts(15)), vec![8, 4, 2, 1]);
    }

    proptest::proptest! {
        #[test]
        fn telescoping_times_cover_total_time_span(now in 0i64..u32::MAX as i64) {
            let topo = Topology::identity(Timestamp::from_micros(0));
            let ts = topo.telescoping_times(Timestamp::from_micros(now));
            let total = ts.iter().fold(0u64, |len, t| {
                assert_eq!(*t.bounds().0, len as u32, "t = {:?}, len = {}", t, len);
                len + t.length()
            });
            assert_eq!(total, now as u64);
        }

        #[test]
        fn telescoping_times_end_with_1(now: i64) {
            let topo = Topology::identity(Timestamp::from_micros(0));
            if let Some(last) = topo.telescoping_times(Timestamp::from_micros(now)).pop() {
                assert_eq!(last.power, 0);
            }
        }

        #[test]
        fn telescoping_times_are_fractal(now: u32) {
            let topo = Topology::identity(Timestamp::from_micros(0));
            let a = lengths(&topo, Timestamp::from_micros(now as i64));
            let b = lengths(&topo, Timestamp::from_micros((now - a[0]) as i64));
            assert_eq!(b.as_slice(), &a[1..]);
        }
    }
}
