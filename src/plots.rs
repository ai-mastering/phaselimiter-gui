use crate::Message;
use iced::widget::{
    Action,
    canvas::{self, Frame, Geometry, Path, Stroke},
};
use iced::{Color, Point, Rectangle, Renderer, Theme, mouse};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Spectrum,
    Loudness,
    Histogram,
}
impl Kind {
    pub const ALL: [Self; 3] = [Self::Spectrum, Self::Loudness, Self::Histogram];
}
impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Spectrum => "Frequency balance",
            Self::Loudness => "Loudness over time",
            Self::Histogram => "Loudness histogram",
        })
    }
}
pub struct Plot {
    pub input: Vec<(f32, f32)>,
    pub output: Vec<(f32, f32)>,
    pub waveform: bool,
    pub logarithmic: bool,
    pub position: f32,
    pub region: Option<(f32, f32)>,
}
impl canvas::Program<Message> for Plot {
    type State = Option<f32>;
    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        if !self.waveform {
            return None;
        }
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let p = cursor.position_in(bounds)?;
                *state = Some((p.x / bounds.width).clamp(0., 1.));
                Some(Action::capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let start = state.take()?;
                let p = cursor
                    .position()
                    .map(|p| ((p.x - bounds.x) / bounds.width).clamp(0., 1.))
                    .unwrap_or(start);
                if (p - start).abs() < 0.008 {
                    Some(Action::publish(Message::SeekFraction(p)))
                } else {
                    Some(Action::publish(Message::Region(start.min(p), start.max(p))))
                }
            }
            _ => None,
        }
    }
    fn draw(
        &self,
        _: &Self::State,
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            Color::from_rgb8(245, 247, 250),
        );
        let transform = |x: f32| {
            if self.logarithmic {
                x.max(20.).log10()
            } else {
                x
            }
        };
        let points: Vec<_> = self
            .input
            .iter()
            .chain(self.output.iter())
            .filter(|(x, _)| !self.logarithmic || *x >= 20.)
            .collect();
        let (xmin, xmax, ymin, ymax) = if self.waveform {
            (0., 1., -1., 1.)
        } else {
            let xmin = points
                .iter()
                .map(|p| transform(p.0))
                .fold(f32::INFINITY, f32::min);
            let xmax = points
                .iter()
                .map(|p| transform(p.0))
                .fold(f32::NEG_INFINITY, f32::max);
            let ymin = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
            let ymax = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
            if points.is_empty() {
                (0., 1., 0., 1.)
            } else {
                (xmin, xmax, ymin, ymax)
            }
        };
        let margin = if self.waveform { 0. } else { 38. };
        let w = (bounds.width - margin - 8.).max(1.);
        let h = (bounds.height - 24.).max(1.);
        for i in 0..=4 {
            let y = 8. + h * i as f32 / 4.;
            frame.stroke(
                &Path::line(Point::new(margin, y), Point::new(margin + w, y)),
                Stroke::default().with_color(Color::from_rgb8(218, 224, 232)),
            );
            if !self.waveform {
                frame.fill_text(canvas::Text {
                    content: format!("{:.0}", ymax - (ymax - ymin) * i as f32 / 4.),
                    position: Point::new(2., y),
                    size: 10.into(),
                    ..Default::default()
                });
                let x = xmin + (xmax - xmin) * i as f32 / 4.;
                frame.fill_text(canvas::Text {
                    content: format!("{:.0}", if self.logarithmic { 10f32.powf(x) } else { x }),
                    position: Point::new(margin + w * i as f32 / 4., h + 9.),
                    size: 10.into(),
                    ..Default::default()
                });
            }
        }
        if self.waveform
            && let Some((start, end)) = self.region
        {
            frame.fill_rectangle(
                Point::new(w * start, 0.),
                iced::Size::new(w * (end - start), bounds.height),
                Color::from_rgba(0.2, 0.5, 0.9, 0.15),
            );
        }
        for (series, color) in [
            (&self.input, Color::from_rgb8(125, 137, 152)),
            (&self.output, Color::from_rgb8(35, 105, 210)),
        ] {
            let path = Path::new(|builder| {
                let mut first = true;
                for &(x, y) in series {
                    if self.logarithmic && x < 20. {
                        continue;
                    }
                    let point = Point::new(
                        margin + w * (transform(x) - xmin) / (xmax - xmin).max(0.001),
                        8. + h * (1. - (y - ymin) / (ymax - ymin).max(0.001)),
                    );
                    if first {
                        builder.move_to(point);
                        first = false;
                    } else {
                        builder.line_to(point);
                    }
                }
            });
            frame.stroke(&path, Stroke::default().with_color(color).with_width(1.5));
        }
        if self.waveform {
            frame.stroke(
                &Path::line(
                    Point::new(w * self.position, 0.),
                    Point::new(w * self.position, bounds.height),
                ),
                Stroke::default()
                    .with_color(Color::from_rgb8(20, 75, 170))
                    .with_width(2.),
            );
        }
        vec![frame.into_geometry()]
    }
}
