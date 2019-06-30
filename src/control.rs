pub struct ControlCurve {
    data_points: Vec<(f64, f64)>,
}

impl ControlCurve {
    pub fn new<I: IntoIterator<Item=(f64, f64)>>(data_points: I) -> ControlCurve {
        ControlCurve {
            data_points: data_points.into_iter().collect()
        }
    }

    pub fn control(&self, input: f64) -> f64 {
        if self.data_points.is_empty() {
            return std::f64::NAN;
        }

        let high_index = self.data_points
            .binary_search_by(|probe| probe.0.partial_cmp(&input).expect("Must not be NaN"))
            .unwrap_or_else(|pos| pos);

        if high_index == 0 {
            // input is below lowest value
            self.data_points[high_index].1
        } else if high_index == self.data_points.len() {
            // input is above highest value
            self.data_points[high_index - 1].1
        } else {
            assert!(high_index > 0 && high_index < self.data_points.len());
            // input is in between two values
            let low_index = high_index - 1;
            let (low_x, low_y) = self.data_points[low_index];
            let (high_x, high_y) = self.data_points[high_index];

            low_y + (high_y - low_y) * (input - low_x) / (high_x - low_x)
        }
    }
}

#[cfg(test)]
mod test {
    use super::ControlCurve;

    fn make_test_curve() -> ControlCurve {
        ControlCurve {
            data_points: vec![(10.0, 5.0), (30.0, 10.), (50.0, 50.), (100.0, 80.)]
        }
    }

    #[test]
    fn control_curve_clamping() {
        let curve = make_test_curve();
        assert_eq!(curve.control(0.0), 5.0);
        assert_eq!(curve.control(110.0), 80.);
    }

    #[test]
    fn control_curve_exact() {
        let curve = make_test_curve();
        assert_eq!(curve.control(10.0), 5.0);
        assert_eq!(curve.control(30.0), 10.0);
        assert_eq!(curve.control(10.0), 5.0);
    }

    #[test]
    fn control_curve_interpolate() {
        let curve = make_test_curve();
        assert_eq!(curve.control(20.0), 7.5);
        assert_eq!(curve.control(45.0), 40.0);
    }
}