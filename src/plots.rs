use crate::Message;
use crate::analysis::Series;
use crate::theme::Theme;
use iced::widget::{
    Action,
    canvas::{self, Frame, Geometry, Path, Stroke},
};
use iced::{Color, Point, Rectangle, Renderer, mouse};
use std::{cell::RefCell, collections::VecDeque, sync::Arc};

const CACHE_ENTRIES: usize = 100;

fn axis_label(value: f32, step: f32) -> String {
    let mut precision = if step > 0. {
        (-step.log10()).ceil().clamp(0., 4.) as usize
    } else {
        0
    };
    while precision < 4 {
        let scaled = step * 10f32.powi(precision as i32);
        if (scaled - scaled.round()).abs() < 0.001 {
            break;
        }
        precision += 1;
    }
    format!("{value:.precision$}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Spectrum,
    Loudness,
    Histogram,
    Distribution,
}
impl Kind {
    pub const ALL: [Self; 4] = [
        Self::Spectrum,
        Self::Loudness,
        Self::Histogram,
        Self::Distribution,
    ];
}
impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Spectrum => "Spectrum",
            Self::Loudness => "Loudness",
            Self::Histogram => "Histogram",
            Self::Distribution => "Distribution",
        })
    }
}
pub struct Plot {
    pub input: Series,
    pub output: Series,
    pub reference: Series,
    pub waveform: bool,
    pub logarithmic: bool,
    pub position: f32,
    pub region: Option<(f32, f32)>,
}
#[derive(Default)]
pub struct State {
    drag: Option<f32>,
    cached: RefCell<VecDeque<CachedPlot>>,
    #[cfg(test)]
    builds: std::cell::Cell<usize>,
}
struct CachedPlot {
    input: Series,
    output: Series,
    reference: Series,
    waveform: bool,
    logarithmic: bool,
    geometry: canvas::Cache,
}
impl CachedPlot {
    fn matches(&self, plot: &Plot) -> bool {
        Arc::ptr_eq(&self.input, &plot.input)
            && Arc::ptr_eq(&self.output, &plot.output)
            && Arc::ptr_eq(&self.reference, &plot.reference)
            && self.waveform == plot.waveform
            && self.logarithmic == plot.logarithmic
    }
}
impl canvas::Program<Message, Theme> for Plot {
    type State = State;
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
                state.drag = Some((p.x / bounds.width).clamp(0., 1.));
                Some(Action::capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let start = state.drag.take()?;
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
        state: &Self::State,
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut cached = state.cached.borrow_mut();
        let entry = cached
            .iter()
            .position(|entry| entry.matches(self))
            .and_then(|index| cached.remove(index))
            .unwrap_or_else(|| CachedPlot {
                input: self.input.clone(),
                output: self.output.clone(),
                reference: self.reference.clone(),
                waveform: self.waveform,
                logarithmic: self.logarithmic,
                geometry: canvas::Cache::new(),
            });
        let base = entry.geometry.draw(renderer, bounds.size(), |frame| {
            #[cfg(test)]
            state.builds.set(state.builds.get() + 1);
            frame.fill_rectangle(
                Point::ORIGIN,
                bounds.size(),
                Color::from_rgb8(245, 247, 250),
            );
            self.draw_static(frame, bounds);
        });
        cached.push_back(entry);
        if cached.len() > CACHE_ENTRIES {
            cached.pop_front();
        }
        let mut layers = vec![base];
        if self.waveform {
            let mut overlay = Frame::new(renderer, bounds.size());
            if let Some((start, end)) = self.region {
                overlay.fill_rectangle(
                    Point::new(bounds.width * start, 0.),
                    iced::Size::new(bounds.width * (end - start), bounds.height),
                    Color::from_rgba(0.2, 0.5, 0.9, 0.15),
                );
            }
            overlay.stroke(
                &Path::line(
                    Point::new(bounds.width * self.position, 0.),
                    Point::new(bounds.width * self.position, bounds.height),
                ),
                Stroke::default()
                    .with_color(Color::from_rgb8(20, 75, 170))
                    .with_width(2.),
            );
            layers.push(overlay.into_geometry());
        }
        layers
    }
}
impl Plot {
    fn draw_static(&self, frame: &mut Frame, bounds: Rectangle) {
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
            .chain(self.reference.iter())
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
        let ylabels: Vec<_> = (0..=4)
            .map(|i| axis_label(ymax - (ymax - ymin) * i as f32 / 4., (ymax - ymin) / 4.))
            .collect();
        let margin = if self.waveform {
            0.
        } else {
            (ylabels.iter().map(String::len).max().unwrap_or(0) as f32 * 7. + 12.).max(38.)
        };
        let w = if self.waveform {
            bounds.width.max(1.)
        } else {
            (bounds.width - margin - 8.).max(1.)
        };
        let top = if self.waveform { 8. } else { 12. };
        let h = (bounds.height - if self.waveform { 24. } else { 40. }).max(1.);
        for (i, label) in ylabels.iter().enumerate() {
            let y = top + h * i as f32 / 4.;
            frame.stroke(
                &Path::line(Point::new(margin, y), Point::new(margin + w, y)),
                Stroke::default().with_color(Color::from_rgb8(218, 224, 232)),
            );
            if !self.waveform {
                frame.fill_text(canvas::Text {
                    content: label.clone(),
                    position: Point::new(margin - 8., y),
                    align_x: iced::alignment::Horizontal::Right.into(),
                    align_y: iced::alignment::Vertical::Center,
                    size: 10.into(),
                    ..Default::default()
                });
                let x = xmin + (xmax - xmin) * i as f32 / 4.;
                frame.fill_text(canvas::Text {
                    content: axis_label(
                        if self.logarithmic { 10f32.powf(x) } else { x },
                        if self.logarithmic {
                            1.
                        } else {
                            (xmax - xmin) / 4.
                        },
                    ),
                    position: Point::new(margin + w * i as f32 / 4., h + 20.),
                    align_x: match i {
                        0 => iced::alignment::Horizontal::Left,
                        4 => iced::alignment::Horizontal::Right,
                        _ => iced::alignment::Horizontal::Center,
                    }
                    .into(),
                    size: 10.into(),
                    ..Default::default()
                });
            }
        }
        for (series, color) in [
            (&self.input, Color::from_rgb8(125, 137, 152)),
            (&self.output, Color::from_rgb8(35, 105, 210)),
            (&self.reference, Color::from_rgb8(190, 120, 20)),
        ] {
            let path = Path::new(|builder| {
                let mut first = true;
                for &(x, y) in series.iter() {
                    if self.logarithmic && x < 20. {
                        continue;
                    }
                    let point = Point::new(
                        margin + w * (transform(x) - xmin) / (xmax - xmin).max(0.001),
                        top + h * (1. - (y - ymin) / (ymax - ymin).max(0.001)),
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plot(input: Series) -> Plot {
        Plot {
            input,
            output: crate::analysis::empty_series(),
            reference: crate::analysis::empty_series(),
            waveform: true,
            logarithmic: false,
            position: 0.,
            region: None,
        }
    }
    #[test]
    fn fractional_axis_ticks_remain_distinct() {
        let labels: Vec<_> = (0..=4).map(|i| axis_label(i as f32 * 0.25, 0.25)).collect();
        assert!(labels.windows(2).all(|pair| pair[0] != pair[1]));
        assert_eq!(axis_label(24000., 1.), "24000");
        assert_eq!(axis_label(-100., 20.), "-100");
        assert_eq!(axis_label(0.025, 0.025), "0.025");
    }
    #[test]
    fn cursor_loop_and_revisiting_a_source_reuse_static_geometry() {
        let renderer = Renderer::new(iced::Font::DEFAULT, iced::Pixels(16.));
        let state = State::default();
        let bounds = Rectangle::with_size(iced::Size::new(800., 100.));
        let mut a = plot(Arc::from([(0., 0.1), (0.5, -0.2), (1., 0.3)]));
        let b = plot(Arc::from([(0., 0.4), (0.5, -0.1), (1., 0.2)]));
        let draw = |plot: &Plot, bounds| {
            <Plot as canvas::Program<Message, Theme>>::draw(
                plot,
                &state,
                &renderer,
                &Theme,
                bounds,
                mouse::Cursor::Unavailable,
            )
        };
        draw(&a, bounds);
        assert_eq!(state.builds.get(), 1);
        a.position = 0.7;
        a.region = Some((0.2, 0.6));
        draw(&a, bounds);
        assert_eq!(state.builds.get(), 1);
        draw(&b, bounds);
        assert_eq!(state.builds.get(), 2);
        draw(&a, bounds);
        assert_eq!(state.builds.get(), 2);
        draw(&a, Rectangle::with_size(iced::Size::new(1000., 100.)));
        assert_eq!(state.builds.get(), 3);
        a.reference = Arc::from([(0., 0.2), (1., 0.5)]);
        draw(&a, bounds);
        assert_eq!(state.builds.get(), 4);
        draw(&a, bounds);
        assert_eq!(state.builds.get(), 4);
    }
    // Render with the application's real canvas renderer for manual visual review.
    #[test]
    #[ignore]
    fn chart_visual_review() {
        let directory = std::env::var("CHART_REVIEW_DIR").expect("set CHART_REVIEW_DIR");
        for (name, logarithmic, waveform, points) in [
            (
                "spectrum",
                true,
                false,
                vec![(20., -100.), (100., -45.), (1000., -20.), (24000., -60.)],
            ),
            (
                "loudness",
                false,
                false,
                vec![(0., -24.), (0.25, -10.), (0.5, -18.), (1., -12.)],
            ),
            (
                "histogram",
                false,
                false,
                vec![(-60., 0.), (-24., 120000.), (-12., 240000.), (0., 0.)],
            ),
            (
                "waveform",
                false,
                true,
                vec![(0., 0.), (0.25, 1.), (0.5, -1.), (1., 0.)],
            ),
        ] {
            for width in [280., 560.] {
                let chart = Plot {
                    logarithmic,
                    waveform,
                    ..plot(Arc::from(points.clone()))
                };
                let mut ui = iced_test::Simulator::<Message, Theme, Renderer>::with_size(
                    Default::default(),
                    iced::Size::new(width, 220.),
                    iced::widget::canvas(chart)
                        .width(iced::Length::Fill)
                        .height(iced::Length::Fill),
                );
                assert!(
                    ui.snapshot(&Theme)
                        .unwrap()
                        .matches_image(format!("{directory}/{name}-{width}.png"))
                        .unwrap()
                );
            }
        }
    }
}
