use eframe::egui::{self, Align2, Color32, FontId, Id, Pos2, Rect, Rgba, Stroke, Vec2};

use crate::api::TokenMetricSample;

const TRACE_LIMIT: usize = 320;
const SPHERE_SEGMENTS: usize = 56;
const SPHERE_LATITUDE_BANDS: usize = 6;
const SPHERE_LONGITUDE_BANDS: usize = 7;

#[derive(Debug, Clone)]
pub struct TokenMonitorState {
    conversation_id: Option<String>,
    trace: Vec<TracePoint>,
    current_position: Vec3,
    current_direction: Vec3,
    sample_index: u64,
    last_novelty: f32,
    zoom: f32,
    target_zoom: f32,
    yaw_offset: f32,
    pitch_offset: f32,
    rotation_velocity: Vec2,
    drag_accum: Vec2,
    auto_yaw: f32,
    auto_pitch_phase: f32,
    last_frame_time: Option<f64>,
    auto_rotate_resume_at: f64,
}

#[derive(Debug, Clone)]
struct TracePoint {
    token: String,
    position: Vec3,
    novelty: f32,
    emphasis: f32,
    logprob: Option<f32>,
    entropy: Option<f32>,
    radius: f32,
    step_length: f32,
}

#[derive(Debug, Clone)]
struct HoverHit {
    screen_pos: Pos2,
    point: TracePoint,
    index: usize,
}

#[derive(Debug, Clone, Copy)]
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

impl TokenMonitorState {
    pub fn new() -> Self {
        Self {
            conversation_id: None,
            trace: Vec::new(),
            current_position: Vec3::ZERO,
            current_direction: Vec3::new(0.22, 0.34, 0.91).normalized(),
            sample_index: 0,
            last_novelty: 0.0,
            zoom: 1.0,
            target_zoom: 1.0,
            yaw_offset: 0.0,
            pitch_offset: 0.0,
            rotation_velocity: Vec2::ZERO,
            drag_accum: Vec2::ZERO,
            auto_yaw: 0.0,
            auto_pitch_phase: 0.0,
            last_frame_time: None,
            auto_rotate_resume_at: 0.0,
        }
    }

    pub fn ingest(&mut self, conversation_id: &str, clear: bool, samples: &[TokenMetricSample]) {
        if clear || self.conversation_id.as_deref() != Some(conversation_id) {
            self.reset(conversation_id);
        }

        for sample in samples {
            self.push_sample(sample);
        }
    }

    pub fn last_novelty(&self) -> f32 {
        self.last_novelty
    }

    pub fn trace_len(&self) -> usize {
        self.trace.len()
    }

    fn reset(&mut self, conversation_id: &str) {
        self.conversation_id = Some(conversation_id.to_string());
        self.trace.clear();
        self.current_position = Vec3::ZERO;
        self.current_direction = Vec3::new(0.22, 0.34, 0.91).normalized();
        self.sample_index = 0;
        self.last_novelty = 0.0;
    }

    fn push_sample(&mut self, sample: &TokenMetricSample) {
        let novelty = sample.novelty.clamp(0.0, 1.35);
        let surprisal = sample
            .logprob
            .map(|value| (-value / 5.5).clamp(0.0, 1.0))
            .unwrap_or(novelty.min(1.0));
        let entropy = sample
            .entropy
            .map(|value| (value / 1.75).clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let turn_strength = (0.14 + 0.52 * novelty.min(1.0) + 0.22 * entropy).clamp(0.12, 0.9);
        let step_length = 0.04 + 0.1 * novelty.min(1.0) + 0.045 * surprisal;
        let target_direction = hashed_direction(&sample.text, self.sample_index);
        let direction = self
            .current_direction
            .lerp(target_direction, turn_strength)
            .normalized();
        let center_pull = (0.965 - 0.03 * novelty.min(1.0)).clamp(0.9, 0.975);

        self.current_direction = direction;
        self.current_position = self.current_position * center_pull + direction * step_length;
        self.last_novelty = novelty;
        self.sample_index += 1;
        self.trace.push(TracePoint {
            token: sample.text.clone(),
            position: self.current_position,
            novelty,
            emphasis: (0.55 * surprisal + 0.45 * entropy).clamp(0.0, 1.0),
            logprob: sample.logprob,
            entropy: sample.entropy,
            radius: self.current_position.length(),
            step_length,
        });

        if self.trace.len() > TRACE_LIMIT {
            let drain = self.trace.len() - TRACE_LIMIT;
            self.trace.drain(0..drain);
        }
    }

    fn reset_view(&mut self) {
        self.zoom = 1.0;
        self.target_zoom = 1.0;
        self.yaw_offset = 0.0;
        self.pitch_offset = 0.0;
        self.rotation_velocity = Vec2::ZERO;
        self.drag_accum = Vec2::ZERO;
        self.auto_rotate_resume_at = 0.0;
    }
}

impl Default for TokenMonitorState {
    fn default() -> Self {
        Self::new()
    }
}

impl Vec3 {
    const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    fn normalized(self) -> Self {
        let length = self.length();
        if length <= f32::EPSILON {
            return Self::new(0.0, 0.0, 1.0);
        }
        self * (1.0 / length)
    }

    fn lerp(self, other: Self, amount: f32) -> Self {
        self * (1.0 - amount) + other * amount
    }
}

impl std::ops::Add for Vec3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl std::ops::Mul<f32> for Vec3 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

pub fn render(ui: &mut egui::Ui, state: &mut TokenMonitorState) {
    let desired_size = egui::vec2(ui.available_width().max(180.0), 220.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let hovered = response.hovered();
    let time = ui.ctx().input(|input| input.time);
    let delta_time = state
        .last_frame_time
        .map(|previous| (time - previous).clamp(0.0, 0.05) as f32)
        .unwrap_or(1.0 / 60.0);
    state.last_frame_time = Some(time);

    painter.rect_filled(rect, 10.0, Color32::BLACK);
    if response.double_clicked() {
        state.reset_view();
    }

    if hovered {
        let scroll_delta = ui.ctx().input(|input| input.raw_scroll_delta.y);
        if scroll_delta.abs() > f32::EPSILON {
            let zoom_factor = (1.0 + scroll_delta * 0.0015).clamp(0.78, 1.22);
            state.target_zoom = (state.target_zoom * zoom_factor).clamp(0.55, 2.8);
        }
    }

    if response.drag_started() {
        state.drag_accum = Vec2::ZERO;
        state.rotation_velocity = Vec2::ZERO;
        state.auto_rotate_resume_at = time + 5.0;
    }
    if response.dragged() {
        let delta = response.drag_delta() - state.drag_accum;
        state.drag_accum = response.drag_delta();
        state.yaw_offset += delta.x * 0.012;
        state.pitch_offset = (state.pitch_offset + delta.y * 0.01).clamp(-1.1, 1.1);
        state.rotation_velocity = delta * 0.0015;
        state.auto_rotate_resume_at = time + 5.0;
    } else {
        state.drag_accum = Vec2::ZERO;
        state.yaw_offset += state.rotation_velocity.x;
        state.pitch_offset = (state.pitch_offset + state.rotation_velocity.y).clamp(-1.1, 1.1);
        state.rotation_velocity *= 0.92;
        if state.rotation_velocity.length_sq() < 0.000_002 {
            state.rotation_velocity = Vec2::ZERO;
        }
    }
    state.zoom += (state.target_zoom - state.zoom) * 0.18;

    if time >= state.auto_rotate_resume_at {
        state.auto_yaw += delta_time * 0.22;
        state.auto_pitch_phase += delta_time * 0.31;
    }
    let yaw = state.auto_yaw + state.yaw_offset;
    let pitch =
        (0.48 + 0.09 * state.auto_pitch_phase.sin() + state.pitch_offset).clamp(-1.1, 1.1);
    let sphere_radius = rect.width().min(rect.height()) * 0.34 * state.zoom;
    let center = rect.center();

    draw_wireframe_sphere(&painter, rect, center, sphere_radius, yaw, pitch);
    if hovered {
        draw_hover_guides(&painter, rect, center, sphere_radius, yaw, pitch);
    }
    draw_origin(&painter, center);
    let hover_hit = draw_trace(
        &painter,
        rect,
        center,
        sphere_radius,
        yaw,
        pitch,
        state,
        response.hover_pos(),
    );
    if let Some(hit) = hover_hit {
        draw_hover_marker(&painter, hit.screen_pos);
        egui::show_tooltip_at_pointer(
            ui.ctx(),
            ui.layer_id(),
            Id::new(("token_monitor_tooltip", hit.index)),
            |ui| {
            ui.label(format!("Token: {}", render_token_label(&hit.point.token)));
            ui.label(format!("Step: #{}", hit.index + 1));
            ui.label(format!("Novelty: {:.3}", hit.point.novelty));
            ui.label(format!("Step length: {:.3}", hit.point.step_length));
            ui.label(format!("Radius: {:.3}", hit.point.radius));
            if let Some(logprob) = hit.point.logprob {
                ui.label(format!("Logprob: {:.3}", logprob));
            }
            if let Some(entropy) = hit.point.entropy {
                ui.label(format!("Entropy: {:.3}", entropy));
            }
            },
        );
    }

    if response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    ui.ctx().request_repaint();
}

fn draw_origin(painter: &egui::Painter, center: Pos2) {
    painter.circle_filled(center, 2.0, Color32::from_rgb(128, 255, 170));
    painter.circle_stroke(
        center,
        5.5,
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(128, 255, 170, 40)),
    );
}

fn draw_wireframe_sphere(
    painter: &egui::Painter,
    rect: Rect,
    center: Pos2,
    radius: f32,
    yaw: f32,
    pitch: f32,
) {
    let base_color = Rgba::from_rgba_unmultiplied(0.72, 1.0, 0.8, 0.42);

    for band in 0..SPHERE_LATITUDE_BANDS {
        let latitude = -0.82 + 1.64 * band as f32 / (SPHERE_LATITUDE_BANDS - 1) as f32;
        let y = latitude.sin();
        let ring_radius = (1.0 - y * y).max(0.05).sqrt();
        let mut points = Vec::with_capacity(SPHERE_SEGMENTS + 1);
        for step in 0..=SPHERE_SEGMENTS {
            let angle = std::f32::consts::TAU * step as f32 / SPHERE_SEGMENTS as f32;
            points.push(Vec3::new(ring_radius * angle.cos(), y, ring_radius * angle.sin()));
        }
        draw_projected_polyline(painter, rect, center, radius, yaw, pitch, &points, base_color, 1.0);
    }

    for band in 0..SPHERE_LONGITUDE_BANDS {
        let longitude = std::f32::consts::TAU * band as f32 / SPHERE_LONGITUDE_BANDS as f32;
        let mut points = Vec::with_capacity(SPHERE_SEGMENTS + 1);
        for step in 0..=SPHERE_SEGMENTS {
            let angle =
                std::f32::consts::PI * step as f32 / SPHERE_SEGMENTS as f32 - std::f32::consts::FRAC_PI_2;
            let ring_radius = angle.cos();
            points.push(Vec3::new(
                ring_radius * longitude.cos(),
                angle.sin(),
                ring_radius * longitude.sin(),
            ));
        }
        draw_projected_polyline(painter, rect, center, radius, yaw, pitch, &points, base_color, 0.85);
    }
}

fn draw_trace(
    painter: &egui::Painter,
    rect: Rect,
    center: Pos2,
    radius: f32,
    yaw: f32,
    pitch: f32,
    state: &TokenMonitorState,
    pointer_pos: Option<Pos2>,
) -> Option<HoverHit> {
    let mut nearest_hit: Option<(f32, HoverHit)> = None;
    for (index, segment) in state.trace.windows(2).enumerate() {
        let age = index as f32 / state.trace.len().max(1) as f32;
        let from = rotate(segment[0].position, yaw, pitch);
        let to = rotate(segment[1].position, yaw, pitch);
        let Some(from_pos) = project(rect, center, radius, from) else {
            continue;
        };
        let Some(to_pos) = project(rect, center, radius, to) else {
            continue;
        };

        let distance = segment[1].position.length();
        let normalized_radius = (distance / 1.4).clamp(0.0, 1.0);
        let color = blend_color(
            Color32::from_rgb(110, 245, 150),
            Color32::from_rgb(255, 90, 72),
            normalized_radius.max(segment[1].novelty.min(1.0) * 0.5),
        );
        let alpha = (0.18 + 0.82 * age).clamp(0.0, 1.0);
        let depth_boost = ((to.z + 1.5) / 3.0).clamp(0.3, 1.0);
        let stroke = Stroke::new(
            1.15 + 1.7 * segment[1].emphasis,
            Color32::from(Rgba::from(color) * (alpha * depth_boost)),
        );
        painter.line_segment([from_pos, to_pos], stroke);

        if let Some(pointer_pos) = pointer_pos {
            let distance = pointer_pos.distance(to_pos);
            if distance <= 12.0
                && nearest_hit
                    .as_ref()
                    .map(|(best_distance, _)| distance < *best_distance)
                    .unwrap_or(true)
            {
                nearest_hit = Some((
                    distance,
                    HoverHit {
                        screen_pos: to_pos,
                        point: segment[1].clone(),
                        index: index + 1,
                    },
                ));
            }
        }
    }
    nearest_hit.map(|(_, hit)| hit)
}

fn draw_hover_marker(painter: &egui::Painter, position: Pos2) {
    painter.circle_filled(position, 3.0, Color32::from_rgb(255, 200, 120));
    painter.circle_stroke(
        position,
        7.0,
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 210, 140, 90)),
    );
}

fn draw_projected_polyline(
    painter: &egui::Painter,
    rect: Rect,
    center: Pos2,
    radius: f32,
    yaw: f32,
    pitch: f32,
    points: &[Vec3],
    base_color: Rgba,
    width: f32,
) {
    for segment in points.windows(2) {
        let from = rotate(segment[0], yaw, pitch);
        let to = rotate(segment[1], yaw, pitch);
        let Some(from_pos) = project(rect, center, radius, from) else {
            continue;
        };
        let Some(to_pos) = project(rect, center, radius, to) else {
            continue;
        };

        let depth = (((from.z + to.z) * 0.5) + 1.4) / 2.8;
        let color = Color32::from(base_color * depth.clamp(0.12, 0.95));
        painter.line_segment([from_pos, to_pos], Stroke::new(width, color));
    }
}

fn draw_hover_guides(
    painter: &egui::Painter,
    rect: Rect,
    center: Pos2,
    radius: f32,
    yaw: f32,
    pitch: f32,
) {
    let guide_color = Color32::from_rgba_unmultiplied(170, 255, 210, 36);
    let axis_color = Color32::from_rgba_unmultiplied(170, 255, 210, 72);
    let font = FontId::monospace(9.0);

    for guide_radius in [0.5_f32, 1.0_f32] {
        painter.circle_stroke(center, radius * guide_radius, Stroke::new(1.0, guide_color));
    }

    for (axis, label) in [
        (Vec3::new(1.0, 0.0, 0.0), "X"),
        (Vec3::new(0.0, 1.0, 0.0), "Y"),
        (Vec3::new(0.0, 0.0, 1.0), "Z"),
    ] {
        let rotated = rotate(axis, yaw, pitch);
        let Some(end_pos) = project(rect, center, radius, rotated) else {
            continue;
        };
        painter.line_segment([center, end_pos], Stroke::new(1.0, axis_color));
        painter.text(
            end_pos + Vec2::new(4.0, -2.0),
            Align2::LEFT_CENTER,
            label,
            font.clone(),
            axis_color,
        );
    }

    let ruler_y = rect.bottom() - 16.0;
    let start = Pos2::new(center.x - radius, ruler_y);
    let end = Pos2::new(center.x + radius, ruler_y);
    painter.line_segment([start, end], Stroke::new(1.0, guide_color));
    for (fraction, label) in [(0.5_f32, "0.5"), (1.0_f32, "1.0")] {
        let x = center.x + radius * fraction;
        painter.line_segment(
            [Pos2::new(x, ruler_y - 4.0), Pos2::new(x, ruler_y + 4.0)],
            Stroke::new(1.0, axis_color),
        );
        painter.text(
            Pos2::new(x, ruler_y - 6.0),
            Align2::CENTER_BOTTOM,
            label,
            font.clone(),
            axis_color,
        );
    }
}

fn rotate(vector: Vec3, yaw: f32, pitch: f32) -> Vec3 {
    let sin_y = yaw.sin();
    let cos_y = yaw.cos();
    let yawed = Vec3::new(
        vector.x * cos_y - vector.z * sin_y,
        vector.y,
        vector.x * sin_y + vector.z * cos_y,
    );

    let sin_x = pitch.sin();
    let cos_x = pitch.cos();
    Vec3::new(
        yawed.x,
        yawed.y * cos_x - yawed.z * sin_x,
        yawed.y * sin_x + yawed.z * cos_x,
    )
}

fn project(rect: Rect, center: Pos2, radius: f32, point: Vec3) -> Option<Pos2> {
    let depth = point.z + 2.7;
    if depth <= 0.1 {
        return None;
    }
    let scale = radius / depth;
    let projected = center + Vec2::new(point.x * scale, point.y * scale * 0.92);
    rect.expand(8.0).contains(projected).then_some(projected)
}

fn hashed_direction(text: &str, sample_index: u64) -> Vec3 {
    let mut seed = 0xcbf29ce484222325u64 ^ sample_index.rotate_left(17);
    for byte in text.as_bytes() {
        seed ^= *byte as u64;
        seed = seed.wrapping_mul(0x100000001b3);
    }
    let a = hash_unit(seed);
    let b = hash_unit(seed.rotate_left(21) ^ 0x9e3779b97f4a7c15);
    let theta = std::f32::consts::TAU * a;
    let z = 2.0 * b - 1.0;
    let radial = (1.0 - z * z).sqrt();
    Vec3::new(radial * theta.cos(), z, radial * theta.sin()).normalized()
}

fn hash_unit(mut value: u64) -> f32 {
    value ^= value >> 33;
    value = value.wrapping_mul(0xff51afd7ed558ccd);
    value ^= value >> 33;
    value = value.wrapping_mul(0xc4ceb9fe1a85ec53);
    value ^= value >> 33;
    (value as f64 / u64::MAX as f64) as f32
}

fn blend_color(from: Color32, to: Color32, amount: f32) -> Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let from = Rgba::from(from);
    let to = Rgba::from(to);
    Color32::from(from * (1.0 - amount) + to * amount)
}

fn render_token_label(token: &str) -> String {
    if token.trim().is_empty() {
        return "\" \"".to_string();
    }
    format!("{token:?}")
}
