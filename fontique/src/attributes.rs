// Copyright 2024 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Properties for specifying font matching attributes.

use core::fmt;

use parlance::{FontStyle, FontWeight, FontWidth};

/// Whether a font query may synthesize the requested font style.
///
/// This is an input to face matching. When synthesis is forbidden, matching
/// must select the closest concrete face instead of selecting a regular face
/// on the assumption that it can be skewed later.
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub enum FontStyleSynthesis {
    /// Face matching may select a face that requires synthetic style.
    #[default]
    Allowed,
    /// Face matching must select a concrete face without synthetic style.
    Forbidden,
}

/// Primary attributes for font matching: [`FontWidth`], [`FontStyle`] and [`FontWeight`].
///
/// These are used to [configure] a [`Query`].
///
/// [configure]: crate::Query::set_attributes
/// [`Query`]: crate::Query
#[derive(Copy, Clone, PartialEq, Default, Debug)]
pub struct Attributes {
    pub width: FontWidth,
    pub style: FontStyle,
    pub weight: FontWeight,
}

impl Attributes {
    /// Creates new attributes from the given width, style and weight.
    pub fn new(width: FontWidth, style: FontStyle, weight: FontWeight) -> Self {
        Self {
            width,
            style,
            weight,
        }
    }
}

impl fmt::Display for Attributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "width: {}, style: {}, weight: {}",
            self.width, self.style, self.weight
        )
    }
}
