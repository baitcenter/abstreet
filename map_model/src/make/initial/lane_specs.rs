use crate::{osm, LaneType};
use serde_derive::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::{fmt, iter};

// (original direction, reversed direction)
pub fn get_lane_types(osm_tags: &BTreeMap<String, String>) -> (Vec<LaneType>, Vec<LaneType>) {
    if let Some(s) = osm_tags.get(osm::SYNTHETIC_LANES) {
        if let Some(spec) = RoadSpec::parse(s.to_string()) {
            return (spec.fwd, spec.back);
        } else {
            panic!("Bad {} RoadSpec: {}", osm::SYNTHETIC_LANES, s);
        }
    }

    let parking_lane_fwd = osm_tags.get(osm::PARKING_LANE_FWD) == Some(&"true".to_string());
    let parking_lane_back = osm_tags.get(osm::PARKING_LANE_BACK) == Some(&"true".to_string());

    // Easy special cases first.
    if osm_tags.get("junction") == Some(&"roundabout".to_string()) {
        return (vec![LaneType::Driving, LaneType::Sidewalk], Vec::new());
    }
    if osm_tags.get(osm::HIGHWAY) == Some(&"footway".to_string()) {
        return (vec![LaneType::Sidewalk], Vec::new());
    }

    // TODO Reversible roads should be handled differently?
    let oneway = osm_tags.get("oneway") == Some(&"yes".to_string())
        || osm_tags.get("oneway") == Some(&"reversible".to_string());

    // How many driving lanes in each direction?
    let num_driving_fwd = if let Some(n) = osm_tags
        .get("lanes:forward")
        .and_then(|num| num.parse::<usize>().ok())
    {
        n
    } else if let Some(n) = osm_tags
        .get("lanes")
        .and_then(|num| num.parse::<usize>().ok())
    {
        if oneway {
            n
        } else if n % 2 == 0 {
            n / 2
        } else {
            // TODO Really, this is ambiguous, but...
            (n / 2).max(1)
        }
    } else {
        // TODO Grrr.
        1
    };
    let num_driving_back = if let Some(n) = osm_tags
        .get("lanes:backward")
        .and_then(|num| num.parse::<usize>().ok())
    {
        n
    } else if let Some(n) = osm_tags
        .get("lanes")
        .and_then(|num| num.parse::<usize>().ok())
    {
        if oneway {
            0
        } else if n % 2 == 0 {
            n / 2
        } else {
            // TODO Really, this is ambiguous, but...
            (n / 2).max(1)
        }
    } else {
        // TODO Grrr.
        if oneway {
            0
        } else {
            1
        }
    };

    let mut fwd_side: Vec<LaneType> = iter::repeat(LaneType::Driving)
        .take(num_driving_fwd)
        .collect();
    let mut back_side: Vec<LaneType> = iter::repeat(LaneType::Driving)
        .take(num_driving_back)
        .collect();

    // TODO Handle bus lanes properly.
    let has_bus_lane = osm_tags.contains_key("bus:lanes");
    if has_bus_lane {
        fwd_side.pop();
        fwd_side.push(LaneType::Bus);
        if !back_side.is_empty() {
            back_side.pop();
            back_side.push(LaneType::Bus);
        }
    }

    if osm_tags.get("cycleway") == Some(&"lane".to_string()) {
        fwd_side.push(LaneType::Biking);
        if !back_side.is_empty() {
            back_side.push(LaneType::Biking);
        }
    } else {
        if osm_tags.get("cycleway:right") == Some(&"lane".to_string()) {
            fwd_side.push(LaneType::Biking);
        }
        if osm_tags.get("cycleway:left") == Some(&"lane".to_string()) {
            back_side.push(LaneType::Biking);
        }
    }

    // TODO Should we warn when one of these has parking assigned to it from the blockface?
    let definitely_no_parking = match osm_tags.get(osm::HIGHWAY) {
        Some(hwy) => hwy.ends_with("_link") || hwy == "motorway",
        None => false,
    };
    if parking_lane_fwd && !definitely_no_parking {
        fwd_side.push(LaneType::Parking);
    }
    if parking_lane_back && !definitely_no_parking && !back_side.is_empty() {
        back_side.push(LaneType::Parking);
    }

    let has_sidewalk = osm_tags.get(osm::HIGHWAY) != Some(&"motorway".to_string())
        && osm_tags.get(osm::HIGHWAY) != Some(&"motorway_link".to_string());
    if has_sidewalk {
        fwd_side.push(LaneType::Sidewalk);
        if oneway {
            // Only residential streets have a sidewalk on the other side of a one-way.
            if osm_tags.get(osm::HIGHWAY) == Some(&"residential".to_string())
                || osm_tags.get("sidewalk") == Some(&"both".to_string())
            {
                back_side.push(LaneType::Sidewalk);
            }
        } else {
            back_side.push(LaneType::Sidewalk);
        }
    }

    (fwd_side, back_side)
}

// This is a convenient way for map_editor to plumb instructions here.
#[derive(Serialize, Deserialize)]
pub struct RoadSpec {
    pub fwd: Vec<LaneType>,
    pub back: Vec<LaneType>,
}

impl fmt::Display for RoadSpec {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for lt in &self.fwd {
            write!(f, "{}", RoadSpec::lt_to_char(*lt))?;
        }
        write!(f, "/")?;
        for lt in &self.back {
            write!(f, "{}", RoadSpec::lt_to_char(*lt))?;
        }
        Ok(())
    }
}

impl RoadSpec {
    pub fn parse(s: String) -> Option<RoadSpec> {
        let mut fwd: Vec<LaneType> = Vec::new();
        let mut back: Vec<LaneType> = Vec::new();
        let mut seen_slash = false;
        for c in s.chars() {
            if !seen_slash && c == '/' {
                seen_slash = true;
            } else if let Some(lt) = RoadSpec::char_to_lt(c) {
                if seen_slash {
                    back.push(lt);
                } else {
                    fwd.push(lt);
                }
            } else {
                return None;
            }
        }
        if seen_slash && (fwd.len() + back.len()) > 0 {
            Some(RoadSpec { fwd, back })
        } else {
            None
        }
    }

    fn lt_to_char(lt: LaneType) -> char {
        match lt {
            LaneType::Driving => 'd',
            LaneType::Parking => 'p',
            LaneType::Sidewalk => 's',
            LaneType::Biking => 'b',
            LaneType::Bus => 'u',
        }
    }

    fn char_to_lt(c: char) -> Option<LaneType> {
        match c {
            'd' => Some(LaneType::Driving),
            'p' => Some(LaneType::Parking),
            's' => Some(LaneType::Sidewalk),
            'b' => Some(LaneType::Biking),
            'u' => Some(LaneType::Bus),
            _ => None,
        }
    }
}
