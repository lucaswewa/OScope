//! The optics: an objective behind the instrument's tube lens, imaging onto
//! the sensor. It gives the pixel's size at the sample, the blur of a layer
//! out of focus, and the depth of field.

use oscope_core::Um;
use oscope_hal::{Objective, ObjectiveCatalogue};

/// The objective and sensor that form an image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Optics {
    /// The objective's numerical aperture.
    pub na: f64,
    /// Its magnification on the instrument, with the tube lens.
    pub magnification: f64,
    /// The sensor's pixel.
    pub sensor_pixel: Um,
    /// The light's wavelength: green, 0.55 µm, unless set.
    pub wavelength: Um,
    /// The refractive index between the objective and the sample: 1, air,
    /// for the instrument's dry objectives.
    pub medium: f64,
}

impl Optics {
    /// `objective` from `catalogue`, imaging onto pixels of side
    /// `sensor_pixel`, in green light, in air.
    pub fn new(catalogue: &ObjectiveCatalogue, objective: &Objective, sensor_pixel: Um) -> Self {
        Self {
            na: objective.na,
            magnification: catalogue.magnification(objective),
            sensor_pixel,
            wavelength: Um(0.55),
            medium: 1.0,
        }
    }

    /// A sensor pixel's side at the sample.
    pub fn pixel_size(&self) -> Um {
        self.sensor_pixel / self.magnification
    }

    /// The blur of a point `dz` from the focal plane, as the standard
    /// deviation of a Gaussian that stands in for the point spread function.
    ///
    /// - **In focus,** diffraction: 0.21 λ / NA, the Gaussian closest to a
    ///   widefield microscope's Airy pattern (Zhang, Zerubia and
    ///   Olivo-Marin, 2007).
    /// - **Out of focus,** geometric optics: the cone of light of half-angle
    ///   θ, with sin θ = NA / n, spreads a point over a disc of radius
    ///   |dz| tan θ, whose standard deviation is half its radius.
    ///
    /// The two add in quadrature.
    pub fn blur(&self, dz: Um) -> Um {
        let diffraction = 0.21 * self.wavelength.0 / self.na;
        let sin = (self.na / self.medium).min(0.999);
        let radius = dz.0.abs() * sin / (1.0 - sin * sin).sqrt();
        Um((diffraction * diffraction + radius * radius / 4.0).sqrt())
    }

    /// The depth of field: λ n / NA² for the wave optics, plus n e / (M NA)
    /// for the sensor's pixel e at magnification M (Inoué and Spring,
    /// *Video Microscopy*, after Berek).
    pub fn depth_of_field(&self) -> Um {
        let (n, na) = (self.medium, self.na);
        let wave = self.wavelength.0 * n / (na * na);
        let pixel = n * self.sensor_pixel.0 / (self.magnification * na);
        Um(wave + pixel)
    }
}
