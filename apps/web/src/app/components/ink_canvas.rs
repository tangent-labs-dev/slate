use crate::app::bindings::{click_by_id, download_data_url, file_to_data_url, ui_prompt};
use crate::models::{InkDocument, InkEmbed, InkEmbedKind, InkPoint, InkStroke, InkTool};
use icondata::{
    LuCircle, LuEraser, LuHand, LuHighlighter, LuMinus, LuMousePointer2, LuPencil,
    LuRectangleHorizontal,
};
use leptos::ev::{KeyboardEvent, PointerEvent, WheelEvent};
use leptos::html::Canvas;
use leptos::prelude::*;
use leptos::web_sys::HtmlInputElement;
use leptos_icons::Icon;
use perfect_freehand::{InputPoint, StrokeOptions, get_stroke};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    CanvasRenderingContext2d, Element, HtmlCanvasElement, HtmlImageElement,
    PointerEvent as WebPointerEvent, window,
};

const MIN_ZOOM: f64 = 0.2;
const MAX_ZOOM: f64 = 4.0;
const INK_IMAGE_UPLOAD_INPUT_ID: &str = "ink-image-upload-input";

#[component]
pub fn InkCanvasModal(
    initial_document: InkDocument,
    on_cancel: Callback<()>,
    on_save: Callback<InkDocument>,
) -> impl IntoView {
    let (tool, set_tool) = signal(InkTool::Pen);
    let (board_name, set_board_name) = signal(initial_document.name.clone());
    let (background, set_background) = signal(sanitize_hex_color(&initial_document.background));
    let (color, set_color) = signal("#e5e7eb".to_string());
    let (stroke_width, set_stroke_width) = signal(3.0f64);
    let (opacity, set_opacity) = signal(1.0f64);
    let (strokes, set_strokes) = signal(initial_document.strokes.clone());
    let (embeds, set_embeds) = signal(initial_document.embeds.clone());
    let (_redo_stack, set_redo_stack) = signal::<Vec<InkStroke>>(Vec::new());
    let (is_drawing, set_is_drawing) = signal(false);
    let (is_panning, set_is_panning) = signal(false);
    let (space_pressed, set_space_pressed) = signal(false);
    let (last_pan_client, set_last_pan_client) = signal::<Option<(f64, f64)>>(None);
    let (draft_points, set_draft_points) = signal::<Vec<InkPoint>>(Vec::new());
    let (shape_start, set_shape_start) = signal::<Option<InkPoint>>(None);
    let (shape_end, set_shape_end) = signal::<Option<InkPoint>>(None);
    let (selection_start, set_selection_start) = signal::<Option<InkPoint>>(None);
    let (selection_rect, set_selection_rect) = signal::<Option<(f64, f64, f64, f64)>>(None);
    let (selected_ids, set_selected_ids) = signal::<HashSet<String>>(HashSet::new());
    let (camera_x, set_camera_x) = signal(0.0f64);
    let (camera_y, set_camera_y) = signal(0.0f64);
    let (zoom, set_zoom) = signal(1.0f64);
    let (selected_embed_id, set_selected_embed_id) = signal::<Option<String>>(None);
    let (dragging_embed_id, set_dragging_embed_id) = signal::<Option<String>>(None);
    let (dragging_embed_offset, set_dragging_embed_offset) = signal((0.0f64, 0.0f64));
    let (dragging_selected_strokes, set_dragging_selected_strokes) = signal(false);
    let (dragging_strokes_last_point, set_dragging_strokes_last_point) =
        signal::<Option<InkPoint>>(None);
    let (resizing_embed_id, set_resizing_embed_id) = signal::<Option<String>>(None);
    let (resizing_start_point, set_resizing_start_point) = signal((0.0f64, 0.0f64));
    let (resizing_start_size, set_resizing_start_size) = signal((0.0f64, 0.0f64));
    let (panel_open, set_panel_open) = signal(false);
    let (touch_points, set_touch_points) = signal::<HashMap<i32, (f64, f64)>>(HashMap::new());
    let (pinch_start_distance, set_pinch_start_distance) = signal(0.0f64);
    let (pinch_start_zoom, set_pinch_start_zoom) = signal(1.0f64);
    let (pinch_anchor_world, set_pinch_anchor_world) = signal((0.0f64, 0.0f64));
    let (is_pinching, set_is_pinching) = signal(false);

    let canvas_ref = NodeRef::<Canvas>::new();
    let canvas_width = initial_document.width.max(1600.0) as u32;
    let canvas_height = initial_document.height.max(900.0) as u32;
    let pixel_ratio = window()
        .map(|w| w.device_pixel_ratio())
        .unwrap_or(1.0)
        .clamp(1.0, 3.0);
    let backing_canvas_width = (canvas_width as f64 * pixel_ratio).round() as u32;
    let backing_canvas_height = (canvas_height as f64 * pixel_ratio).round() as u32;

    let to_world_from_client =
        move |client_x: f64, client_y: f64, pressure: f64| -> Option<InkPoint> {
        let canvas = canvas_ref.get()?;
        let rect = canvas.get_bounding_client_rect();
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return None;
        }
        let logical_width = canvas.width() as f64 / pixel_ratio;
        let logical_height = canvas.height() as f64 / pixel_ratio;
        let sx = (client_x - rect.left()) * logical_width / rect.width();
        let sy = (client_y - rect.top()) * logical_height / rect.height();
        let (wx, wy) = screen_to_world(
            sx,
            sy,
            logical_width,
            logical_height,
            camera_x.get_untracked(),
            camera_y.get_untracked(),
            zoom.get_untracked(),
        );
        Some(InkPoint {
            x: wx,
            y: wy,
            pressure,
        })
    };
    let to_world_point = move |ev: &PointerEvent| -> Option<InkPoint> {
        to_world_from_client(
            ev.client_x() as f64,
            ev.client_y() as f64,
            pointer_pressure(ev),
        )
    };
    let coalesced_world_points = move |ev: &PointerEvent| -> Vec<InkPoint> {
        let mut out = Vec::new();
        let coalesced = ev.get_coalesced_events();
        let len = coalesced.length();
        if len > 0 {
            for idx in 0..len {
                if let Ok(raw) = coalesced.get(idx).dyn_into::<WebPointerEvent>()
                    && let Some(point) = to_world_from_client(
                        raw.client_x() as f64,
                        raw.client_y() as f64,
                        pointer_pressure(&raw),
                    )
                {
                    out.push(point);
                }
            }
        }
        if out.is_empty()
            && let Some(point) = to_world_from_client(
                ev.client_x() as f64,
                ev.client_y() as f64,
                pointer_pressure(ev),
            )
        {
            out.push(point);
        }
        out
    };
    let client_to_screen = move |client_x: f64, client_y: f64| -> Option<(f64, f64, f64, f64)> {
        let canvas = canvas_ref.get()?;
        let rect = canvas.get_bounding_client_rect();
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return None;
        }
        let width = canvas.width() as f64 / pixel_ratio;
        let height = canvas.height() as f64 / pixel_ratio;
        let sx = (client_x - rect.left()) * width / rect.width();
        let sy = (client_y - rect.top()) * height / rect.height();
        Some((sx, sy, width, height))
    };

    let redraw = move || {
        let Some(canvas) = canvas_ref.get() else {
            return;
        };
        draw_scene(
            &canvas,
            &strokes.get(),
            &embeds.get(),
            tool.get(),
            &draft_points.get(),
            shape_start.get(),
            shape_end.get(),
            selection_rect.get(),
            &selected_ids.get(),
            camera_x.get(),
            camera_y.get(),
            zoom.get(),
            pixel_ratio,
        );
    };

    Effect::new(move |_| {
        let _ = strokes.get();
        let _ = embeds.get();
        let _ = draft_points.get();
        let _ = shape_start.get();
        let _ = shape_end.get();
        let _ = selection_rect.get();
        let _ = selected_ids.get();
        let _ = camera_x.get();
        let _ = camera_y.get();
        let _ = zoom.get();
        let _ = selected_embed_id.get();
        redraw();
    });

    let erase_at = move |point: InkPoint| {
        let mut changed = false;
        let zoom_level = zoom.get_untracked();
        set_strokes.update(|all| {
            let before = all.len();
            all.retain(|stroke| !stroke_hit_test(stroke, point, zoom_level));
            changed = before != all.len();
        });
        if changed {
            set_redo_stack.set(Vec::new());
        }
    };

    let commit_stroke = move |mut stroke: InkStroke| {
        stroke.z_index = next_stroke_z(&strokes.get_untracked());
        set_strokes.update(|all| all.push(stroke));
        set_redo_stack.set(Vec::new());
    };

    let on_embed_pointer_down = move |ev: PointerEvent, embed_id: String| {
        ev.prevent_default();
        ev.stop_propagation();
        if let Some(target) = ev
            .current_target()
            .and_then(|t| t.dyn_into::<Element>().ok())
        {
            let _ = target.set_pointer_capture(ev.pointer_id());
        }
        let Some(point) = to_world_point(&ev) else {
            return;
        };
        if let Some(embed) = embeds
            .get_untracked()
            .into_iter()
            .find(|item| item.id == embed_id)
        {
            set_selected_embed_id.set(Some(embed.id.clone()));
            set_selected_ids.set(HashSet::new());
            set_dragging_embed_id.set(Some(embed.id));
            set_dragging_embed_offset.set((point.x - embed.x, point.y - embed.y));
        }
    };

    let on_embed_resize_pointer_down = move |ev: PointerEvent, embed_id: String| {
        ev.prevent_default();
        ev.stop_propagation();
        if let Some(target) = ev
            .current_target()
            .and_then(|t| t.dyn_into::<Element>().ok())
        {
            let _ = target.set_pointer_capture(ev.pointer_id());
        }
        let Some(point) = to_world_point(&ev) else {
            return;
        };
        if let Some(embed) = embeds
            .get_untracked()
            .into_iter()
            .find(|item| item.id == embed_id)
        {
            set_selected_embed_id.set(Some(embed.id.clone()));
            set_selected_ids.set(HashSet::new());
            set_resizing_embed_id.set(Some(embed.id));
            set_resizing_start_point.set((point.x, point.y));
            set_resizing_start_size.set((embed.width, embed.height));
        }
    };

    let on_pointer_down = move |ev: PointerEvent| {
        ev.prevent_default();
        ev.stop_propagation();
        if ev.pointer_type() == "touch" {
            set_touch_points.update(|points| {
                points.insert(ev.pointer_id(), (ev.client_x() as f64, ev.client_y() as f64));
            });
            let active = touch_points.get_untracked();
            if active.len() >= 2 {
                let mut iter = active.values();
                if let (Some(a), Some(b)) = (iter.next(), iter.next()) {
                    let dx = b.0 - a.0;
                    let dy = b.1 - a.1;
                    let distance = (dx * dx + dy * dy).sqrt().max(1.0);
                    let mid_x = (a.0 + b.0) * 0.5;
                    let mid_y = (a.1 + b.1) * 0.5;
                    if let Some((sx, sy, width, height)) = client_to_screen(mid_x, mid_y) {
                        let current_zoom = zoom.get_untracked();
                        let (anchor_x, anchor_y) = screen_to_world(
                            sx,
                            sy,
                            width,
                            height,
                            camera_x.get_untracked(),
                            camera_y.get_untracked(),
                            current_zoom,
                        );
                        set_pinch_anchor_world.set((anchor_x, anchor_y));
                        set_pinch_start_distance.set(distance);
                        set_pinch_start_zoom.set(current_zoom);
                        set_is_pinching.set(true);
                        set_is_drawing.set(false);
                        set_draft_points.set(Vec::new());
                        set_shape_start.set(None);
                        set_shape_end.set(None);
                        set_selection_start.set(None);
                        set_selection_rect.set(None);
                        set_dragging_selected_strokes.set(false);
                        set_dragging_strokes_last_point.set(None);
                        return;
                    }
                }
            }
        }
        if let Some(target) = ev
            .current_target()
            .and_then(|t| t.dyn_into::<Element>().ok())
        {
            let _ = target.set_pointer_capture(ev.pointer_id());
        }
        let button = ev.button();
        let pan_mode = matches!(tool.get_untracked(), InkTool::Pinch)
            || button == 1
            || (button == 0 && space_pressed.get_untracked());
        if pan_mode {
            set_is_panning.set(true);
            set_last_pan_client.set(Some((ev.client_x() as f64, ev.client_y() as f64)));
            return;
        }

        let Some(point) = to_world_point(&ev) else {
            return;
        };
        set_selected_embed_id.set(None);
        if matches!(tool.get_untracked(), InkTool::Select) {
            if let Some(hit_id) =
                pick_stroke_at_point(&strokes.get_untracked(), point, zoom.get_untracked())
            {
                set_selected_ids.set(std::iter::once(hit_id).collect());
                set_dragging_selected_strokes.set(true);
                set_dragging_strokes_last_point.set(Some(point));
                set_is_drawing.set(false);
                return;
            }
            set_selected_ids.set(HashSet::new());
        }
        set_is_drawing.set(true);
        match tool.get_untracked() {
            InkTool::Pen | InkTool::Highlighter => set_draft_points.set(vec![point]),
            InkTool::Eraser => erase_at(point),
            InkTool::Pinch => {}
            InkTool::Line | InkTool::Rectangle | InkTool::Circle => {
                set_shape_start.set(Some(point));
                set_shape_end.set(Some(point));
            }
            InkTool::Select => {
                set_selection_start.set(Some(point));
                set_selection_rect.set(Some((point.x, point.y, point.x, point.y)));
            }
        }
    };

    let on_pointer_move = move |ev: PointerEvent| {
        if ev.pointer_type() == "touch" {
            set_touch_points.update(|points| {
                if points.contains_key(&ev.pointer_id()) {
                    points.insert(ev.pointer_id(), (ev.client_x() as f64, ev.client_y() as f64));
                }
            });
            let active = touch_points.get_untracked();
            if is_pinching.get_untracked() || active.len() >= 2 {
                ev.prevent_default();
                if active.len() >= 2 {
                    let mut iter = active.values();
                    if let (Some(a), Some(b)) = (iter.next(), iter.next()) {
                        let dx = b.0 - a.0;
                        let dy = b.1 - a.1;
                        let distance = (dx * dx + dy * dy).sqrt().max(1.0);
                        let mid_x = (a.0 + b.0) * 0.5;
                        let mid_y = (a.1 + b.1) * 0.5;
                        if let Some((sx, sy, width, height)) = client_to_screen(mid_x, mid_y) {
                            let start_distance = pinch_start_distance.get_untracked().max(1.0);
                            let start_zoom = pinch_start_zoom.get_untracked();
                            let next_zoom =
                                (start_zoom * (distance / start_distance)).clamp(MIN_ZOOM, MAX_ZOOM);
                            let (anchor_x, anchor_y) = pinch_anchor_world.get_untracked();
                            set_zoom.set(next_zoom);
                            set_camera_x.set(anchor_x - (sx - width * 0.5) / next_zoom);
                            set_camera_y.set(anchor_y - (sy - height * 0.5) / next_zoom);
                            set_is_pinching.set(true);
                        }
                    }
                }
                return;
            }
        }
        if resizing_embed_id.get_untracked().is_some()
            || dragging_embed_id.get_untracked().is_some()
            || dragging_selected_strokes.get_untracked()
            || is_panning.get_untracked()
            || is_drawing.get_untracked()
        {
            ev.prevent_default();
        }
        if let Some(embed_id) = resizing_embed_id.get_untracked() {
            let Some(point) = to_world_point(&ev) else {
                return;
            };
            let (sx, sy) = resizing_start_point.get_untracked();
            let (sw, sh) = resizing_start_size.get_untracked();
            let next_w = (sw + (point.x - sx)).max(120.0);
            let next_h = (sh + (point.y - sy)).max(80.0);
            set_embeds.update(|all| {
                if let Some(embed) = all.iter_mut().find(|item| item.id == embed_id) {
                    embed.width = next_w;
                    embed.height = next_h;
                }
            });
            return;
        }
        if let Some(embed_id) = dragging_embed_id.get_untracked() {
            let Some(point) = to_world_point(&ev) else {
                return;
            };
            let (ox, oy) = dragging_embed_offset.get_untracked();
            set_embeds.update(|all| {
                if let Some(embed) = all.iter_mut().find(|item| item.id == embed_id) {
                    embed.x = point.x - ox;
                    embed.y = point.y - oy;
                }
            });
            return;
        }
        if dragging_selected_strokes.get_untracked() {
            let Some(point) = to_world_point(&ev) else {
                return;
            };
            let Some(last) = dragging_strokes_last_point.get_untracked() else {
                set_dragging_strokes_last_point.set(Some(point));
                return;
            };
            let dx = point.x - last.x;
            let dy = point.y - last.y;
            if dx.abs() > 0.0 || dy.abs() > 0.0 {
                let selected = selected_ids.get_untracked();
                if !selected.is_empty() {
                    set_strokes.update(|all| {
                        for stroke in all.iter_mut() {
                            if selected.contains(&stroke.id) {
                                for p in &mut stroke.points {
                                    p.x += dx;
                                    p.y += dy;
                                }
                            }
                        }
                    });
                }
            }
            set_dragging_strokes_last_point.set(Some(point));
            return;
        }
        if is_panning.get_untracked() {
            if let Some((last_x, last_y)) = last_pan_client.get_untracked() {
                let dx = ev.client_x() as f64 - last_x;
                let dy = ev.client_y() as f64 - last_y;
                let z = zoom.get_untracked();
                set_camera_x.update(|x| *x -= dx / z);
                set_camera_y.update(|y| *y -= dy / z);
                set_last_pan_client.set(Some((ev.client_x() as f64, ev.client_y() as f64)));
            }
            return;
        }

        if !is_drawing.get_untracked() {
            return;
        }
        let Some(point) = to_world_point(&ev) else {
            return;
        };
        match tool.get_untracked() {
            InkTool::Pen | InkTool::Highlighter => {
                let zoom_level = zoom.get_untracked().max(0.2);
                let min_distance_sq = (0.08 / zoom_level).powi(2);
                let samples = coalesced_world_points(&ev);
                set_draft_points.update(|points| {
                    for sample in samples {
                        let should_push = points.last().is_none_or(|last| {
                            let dx = sample.x - last.x;
                            let dy = sample.y - last.y;
                            (dx * dx + dy * dy) >= min_distance_sq
                        });
                        if should_push {
                            points.push(sample);
                        }
                    }
                })
            }
            InkTool::Eraser => erase_at(point),
            InkTool::Pinch => {}
            InkTool::Line | InkTool::Rectangle | InkTool::Circle => set_shape_end.set(Some(point)),
            InkTool::Select => {
                if let Some(start) = selection_start.get_untracked() {
                    set_selection_rect.set(Some((start.x, start.y, point.x, point.y)));
                }
            }
        }
    };

    let on_pointer_up = move |ev: PointerEvent| {
        if ev.pointer_type() == "touch" {
            let was_pinching = is_pinching.get_untracked();
            set_touch_points.update(|points| {
                points.remove(&ev.pointer_id());
            });
            if touch_points.get_untracked().len() < 2 {
                set_is_pinching.set(false);
                set_pinch_start_distance.set(0.0);
            }
            if was_pinching {
                return;
            }
        }
        if let Some(target) = ev
            .current_target()
            .and_then(|t| t.dyn_into::<Element>().ok())
        {
            let _ = target.release_pointer_capture(ev.pointer_id());
        }
        if resizing_embed_id.get_untracked().is_some() {
            set_resizing_embed_id.set(None);
            return;
        }
        if dragging_embed_id.get_untracked().is_some() {
            set_dragging_embed_id.set(None);
            return;
        }
        if dragging_selected_strokes.get_untracked() {
            set_dragging_selected_strokes.set(false);
            set_dragging_strokes_last_point.set(None);
            return;
        }
        if is_panning.get_untracked() {
            set_is_panning.set(false);
            set_last_pan_client.set(None);
            return;
        }
        if !is_drawing.get_untracked() {
            return;
        }
        set_is_drawing.set(false);
        match tool.get_untracked() {
            InkTool::Pen | InkTool::Highlighter => {
                let points = draft_points.get_untracked();
                if points.len() > 1 {
                    let is_highlighter = matches!(tool.get_untracked(), InkTool::Highlighter);
                    commit_stroke(InkStroke {
                        id: Uuid::new_v4().to_string(),
                        tool: if is_highlighter {
                            InkTool::Highlighter
                        } else {
                            InkTool::Pen
                        },
                        color: color.get_untracked(),
                        width: stroke_width.get_untracked(),
                        opacity: if is_highlighter {
                            0.35
                        } else {
                            opacity.get_untracked()
                        },
                        points,
                        z_index: 0,
                    });
                }
                set_draft_points.set(Vec::new());
            }
            InkTool::Line | InkTool::Rectangle | InkTool::Circle => {
                if let (Some(start), Some(end)) =
                    (shape_start.get_untracked(), shape_end.get_untracked())
                {
                    commit_stroke(InkStroke {
                        id: Uuid::new_v4().to_string(),
                        tool: tool.get_untracked(),
                        color: color.get_untracked(),
                        width: stroke_width.get_untracked(),
                        opacity: opacity.get_untracked(),
                        points: vec![start, end],
                        z_index: 0,
                    });
                }
                set_shape_start.set(None);
                set_shape_end.set(None);
            }
            InkTool::Select => {
                if let Some((x1, y1, x2, y2)) = selection_rect.get_untracked() {
                    let (left, right) = (x1.min(x2), x1.max(x2));
                    let (top, bottom) = (y1.min(y2), y1.max(y2));
                    let next_selected = strokes
                        .get_untracked()
                        .into_iter()
                        .filter(|stroke| {
                            let (sx1, sy1, sx2, sy2) = stroke_bounds(stroke);
                            !(sx2 < left || sx1 > right || sy2 < top || sy1 > bottom)
                        })
                        .map(|stroke| stroke.id)
                        .collect::<HashSet<_>>();
                    set_selected_ids.set(next_selected);
                }
                set_selection_start.set(None);
                set_selection_rect.set(None);
            }
            InkTool::Eraser | InkTool::Pinch => {}
        }
    };

    let on_wheel = move |ev: WheelEvent| {
        ev.prevent_default();
        let Some(canvas) = canvas_ref.get() else {
            return;
        };
        let rect = canvas.get_bounding_client_rect();
        let logical_width = canvas.width() as f64 / pixel_ratio;
        let logical_height = canvas.height() as f64 / pixel_ratio;
        let sx = (ev.client_x() as f64 - rect.left()) * logical_width / rect.width();
        let sy = (ev.client_y() as f64 - rect.top()) * logical_height / rect.height();
        let old_zoom = zoom.get_untracked();
        let step = if ev.delta_y() < 0.0 { 1.12 } else { 0.89 };
        let new_zoom = (old_zoom * step).clamp(MIN_ZOOM, MAX_ZOOM);
        let (before_x, before_y) = screen_to_world(
            sx,
            sy,
            logical_width,
            logical_height,
            camera_x.get_untracked(),
            camera_y.get_untracked(),
            old_zoom,
        );
        set_zoom.set(new_zoom);
        let cam_x = before_x - (sx - logical_width * 0.5) / new_zoom;
        let cam_y = before_y - (sy - logical_height * 0.5) / new_zoom;
        set_camera_x.set(cam_x);
        set_camera_y.set(cam_y);
    };

    let on_key_down = move |ev: KeyboardEvent| {
        if keyboard_event_targets_text_input(&ev) {
            return;
        }
        if !(ev.meta_key() || ev.ctrl_key() || ev.alt_key()) {
            match ev.key().to_ascii_lowercase().as_str() {
                "v" => set_tool.set(InkTool::Select),
                "p" => set_tool.set(InkTool::Pen),
                "h" => set_tool.set(InkTool::Highlighter),
                "e" => set_tool.set(InkTool::Eraser),
                "m" => set_tool.set(InkTool::Pinch),
                "l" => set_tool.set(InkTool::Line),
                "r" => set_tool.set(InkTool::Rectangle),
                "c" => set_tool.set(InkTool::Circle),
                "i" => click_by_id(INK_IMAGE_UPLOAD_INPUT_ID),
                "=" | "+" => set_zoom.update(|z| *z = (*z * 1.12).clamp(MIN_ZOOM, MAX_ZOOM)),
                "-" => set_zoom.update(|z| *z = (*z * 0.89).clamp(MIN_ZOOM, MAX_ZOOM)),
                "0" => {
                    set_zoom.set(1.0);
                    set_camera_x.set(0.0);
                    set_camera_y.set(0.0);
                }
                "backspace" | "delete" => {
                    if let Some(embed_id) = selected_embed_id.get_untracked() {
                        set_embeds.update(|all| all.retain(|embed| embed.id != embed_id));
                        set_selected_embed_id.set(None);
                        return;
                    }
                    let selected = selected_ids.get_untracked();
                    if !selected.is_empty() {
                        set_strokes
                            .update(|all| all.retain(|stroke| !selected.contains(&stroke.id)));
                        set_selected_ids.set(HashSet::new());
                        set_redo_stack.set(Vec::new());
                    }
                }
                _ => {}
            }
        }
        if ev.key() == " " {
            ev.prevent_default();
            set_space_pressed.set(true);
        }
        if (ev.meta_key() || ev.ctrl_key()) && ev.key().eq_ignore_ascii_case("z") {
            ev.prevent_default();
            let mut popped: Option<InkStroke> = None;
            set_strokes.update(|all| popped = all.pop());
            if let Some(stroke) = popped {
                set_redo_stack.update(|redo| redo.push(stroke));
            }
        }
        if (ev.meta_key() || ev.ctrl_key()) && ev.key().eq_ignore_ascii_case("y") {
            ev.prevent_default();
            let mut restored: Option<InkStroke> = None;
            set_redo_stack.update(|redo| restored = redo.pop());
            if let Some(stroke) = restored {
                set_strokes.update(|all| all.push(stroke));
            }
        }
    };

    let on_key_up = move |ev: KeyboardEvent| {
        if ev.key() == " " {
            set_space_pressed.set(false);
        }
    };

    let on_undo = move |_| {
        let mut popped: Option<InkStroke> = None;
        set_strokes.update(|all| popped = all.pop());
        if let Some(stroke) = popped {
            set_redo_stack.update(|redo| redo.push(stroke));
        }
    };

    let on_redo = move |_| {
        let mut restored: Option<InkStroke> = None;
        set_redo_stack.update(|redo| restored = redo.pop());
        if let Some(stroke) = restored {
            set_strokes.update(|all| all.push(stroke));
        }
    };

    let on_delete_selected = move |_| {
        if let Some(embed_id) = selected_embed_id.get_untracked() {
            set_embeds.update(|all| all.retain(|embed| embed.id != embed_id));
            set_selected_embed_id.set(None);
            return;
        }
        let selected = selected_ids.get_untracked();
        if selected.is_empty() {
            return;
        }
        set_strokes.update(|all| all.retain(|stroke| !selected.contains(&stroke.id)));
        set_selected_ids.set(HashSet::new());
        set_redo_stack.set(Vec::new());
    };

    let on_clear = move |_| {
        set_strokes.set(Vec::new());
        set_redo_stack.set(Vec::new());
        set_selected_ids.set(HashSet::new());
        set_selected_embed_id.set(None);
    };

    let add_image_embed = move |src: String| {
        let clean_src = src.trim().to_string();
        if clean_src.is_empty() {
            return;
        }
        if !(clean_src.starts_with("https://")
            || clean_src.starts_with("http://")
            || clean_src.starts_with("slate-media://")
            || clean_src.starts_with("data:image/"))
        {
            return;
        }
        let (w, h) = (520.0, 320.0);
        let next_z = embeds
            .get_untracked()
            .iter()
            .map(|embed| embed.z_index)
            .max()
            .unwrap_or(0)
            + 1;
        let embed_id = Uuid::new_v4().to_string();
        set_embeds.update(|all| {
            all.push(InkEmbed {
                id: embed_id.clone(),
                kind: InkEmbedKind::Image,
                src: clean_src,
                x: camera_x.get_untracked() - w * 0.5,
                y: camera_y.get_untracked() - h * 0.5,
                width: w,
                height: h,
                z_index: next_z,
            });
        });
        set_selected_embed_id.set(Some(embed_id));
    };

    let on_add_image = move |_| {
        let src = ui_prompt("Image URL (http/https or slate-media://)", "https://");
        add_image_embed(src);
    };

    let on_click_upload_image = move |_| {
        click_by_id(INK_IMAGE_UPLOAD_INPUT_ID);
    };

    let on_upload_image = move |ev| {
        let input = event_target::<HtmlInputElement>(&ev);
        let file = input.files().and_then(|files| files.get(0));
        input.set_value("");
        let Some(file) = file else { return };
        if !file.type_().starts_with("image/") {
            return;
        }
        let add_image_embed = add_image_embed;
        spawn_local(async move {
            let promise = file_to_data_url(file);
            if let Ok(value) = JsFuture::from(promise).await
                && let Some(data_url) = value.as_string()
            {
                add_image_embed(data_url);
            }
        });
    };

    let on_remove_embed = move |embed_id: String| {
        set_embeds.update(|all| all.retain(|item| item.id != embed_id));
        if selected_embed_id.get_untracked().as_deref() == Some(embed_id.as_str()) {
            set_selected_embed_id.set(None);
        }
    };

    let on_reset_view = move |_| {
        set_zoom.set(1.0);
        set_camera_x.set(0.0);
        set_camera_y.set(0.0);
    };

    let on_zoom_in = move |_| {
        set_zoom.update(|z| *z = (*z * 1.12).clamp(MIN_ZOOM, MAX_ZOOM));
    };

    let on_zoom_out = move |_| {
        set_zoom.update(|z| *z = (*z * 0.89).clamp(MIN_ZOOM, MAX_ZOOM));
    };

    let on_export_png = move |_| {
        let Some(canvas) = canvas_ref.get() else {
            return;
        };
        if let Ok(url) = canvas.to_data_url_with_type("image/png") {
            download_data_url("whiteboard-drawing.png", &url);
        }
    };

    let initial_document_for_save = initial_document.clone();
    let on_save_click = move |_| {
        let mut doc = initial_document_for_save.clone();
        doc.strokes_on_top = true;
        doc.name = sanitize_ink_name(&board_name.get_untracked());
        doc.background = background.get_untracked();
        doc.strokes = strokes.get_untracked();
        doc.embeds = embeds.get_untracked();
        doc.width = canvas_width as f64;
        doc.height = canvas_height as f64;
        let thumbnail =
            render_thumbnail_data_url(&doc.strokes, &doc.embeds, &doc.background).or_else(|| {
            canvas_ref
                .get()
                .and_then(|canvas| canvas.to_data_url_with_type("image/png").ok())
        });
        if thumbnail.is_some() {
            doc.thumbnail_data_url = thumbnail;
        }
        on_save.run(doc);
    };

    view! {
        <div class="ink-modal-backdrop">
            <div class="ink-modal" tabindex="0" on:keydown=on_key_down on:keyup=on_key_up>
                <div class="ink-ui-top">
                    <div class="ink-tool-dock">
                        <button title="Select and move objects. Shortcut: V" class=move || if tool.get() == InkTool::Select { "mode-btn ink-tool-btn ink-tool-icon active" } else { "mode-btn ink-tool-btn ink-tool-icon" } on:click=move |_| set_tool.set(InkTool::Select)>
                            <Icon icon=LuMousePointer2 />
                        </button>
                        <button title="Pan board by dragging. Shortcut: M" class=move || if tool.get() == InkTool::Pinch { "mode-btn ink-tool-btn ink-tool-icon active" } else { "mode-btn ink-tool-btn ink-tool-icon" } on:click=move |_| set_tool.set(InkTool::Pinch)>
                            <Icon icon=LuHand />
                        </button>
                        <button title="Freehand pen stroke. Shortcut: P" class=move || if tool.get() == InkTool::Pen { "mode-btn ink-tool-btn ink-tool-icon active" } else { "mode-btn ink-tool-btn ink-tool-icon" } on:click=move |_| set_tool.set(InkTool::Pen)>
                            <Icon icon=LuPencil />
                        </button>
                        <button title="Transparent highlight stroke. Shortcut: H" class=move || if tool.get() == InkTool::Highlighter { "mode-btn ink-tool-btn ink-tool-icon active" } else { "mode-btn ink-tool-btn ink-tool-icon" } on:click=move |_| set_tool.set(InkTool::Highlighter)>
                            <Icon icon=LuHighlighter />
                        </button>
                        <button title="Erase strokes under cursor. Shortcut: E" class=move || if tool.get() == InkTool::Eraser { "mode-btn ink-tool-btn ink-tool-icon active" } else { "mode-btn ink-tool-btn ink-tool-icon" } on:click=move |_| set_tool.set(InkTool::Eraser)>
                            <Icon icon=LuEraser />
                        </button>
                        <button title="Draw a straight line. Shortcut: L" class=move || if tool.get() == InkTool::Line { "mode-btn ink-tool-btn ink-tool-icon active" } else { "mode-btn ink-tool-btn ink-tool-icon" } on:click=move |_| set_tool.set(InkTool::Line)>
                            <Icon icon=LuMinus />
                        </button>
                        <button title="Draw a rectangle. Shortcut: R" class=move || if tool.get() == InkTool::Rectangle { "mode-btn ink-tool-btn ink-tool-icon active" } else { "mode-btn ink-tool-btn ink-tool-icon" } on:click=move |_| set_tool.set(InkTool::Rectangle)>
                            <Icon icon=LuRectangleHorizontal />
                        </button>
                        <button title="Draw a circle. Shortcut: C" class=move || if tool.get() == InkTool::Circle { "mode-btn ink-tool-btn ink-tool-icon active" } else { "mode-btn ink-tool-btn ink-tool-icon" } on:click=move |_| set_tool.set(InkTool::Circle)>
                            <Icon icon=LuCircle />
                        </button>
                    </div>
                    <div class="ink-actions-dock">
                        <label class="ink-name-field" title="Whiteboard name">
                            <span>"Name"</span>
                            <input
                                class="ink-name-input"
                                type="text"
                                maxlength="80"
                                prop:value=move || board_name.get()
                                on:input=move |ev| set_board_name.set(event_target_value(&ev))
                                placeholder="Whiteboard"
                            />
                        </label>
                        <span class="ink-zoom-badge">{move || format!("{:.0}%", zoom.get() * 100.0)}</span>
                        <button class="mode-btn ink-action-btn" title="Undo (Cmd/Ctrl+Z)" on:click=on_undo>"Undo"</button>
                        <button class="mode-btn ink-action-btn" title="Redo (Cmd/Ctrl+Y)" on:click=on_redo>"Redo"</button>
                        <button class="mode-btn ink-action-btn" on:click=on_export_png>"Export PNG"</button>
                        <button class="mode-btn ink-action-btn" on:click=move |_| on_cancel.run(())>"Close"</button>
                        <button class="mode-btn ink-action-btn active" on:click=on_save_click>"Save"</button>
                    </div>
                </div>

                <div class=move || {
                    if panel_open.get() {
                        "ink-left-panel open".to_string()
                    } else {
                        "ink-left-panel".to_string()
                    }
                }>
                    <div class="ink-panel-title">"Stroke"</div>
                    <label class="ink-slider-label">
                        "Background"
                        <input
                            class="ink-color"
                            type="color"
                            prop:value=move || background.get()
                            on:input=move |ev| set_background.set(sanitize_hex_color(&event_target_value(&ev)))
                        />
                    </label>
                    <label class="ink-slider-label">
                        "Color"
                        <input
                            class="ink-color"
                            type="color"
                            prop:value=move || color.get()
                            on:input=move |ev| set_color.set(event_target_value(&ev))
                        />
                    </label>
                    <div class="ink-color-swatches">
                        <button class="ink-swatch" style="--swatch:#f8fafc;" on:click=move |_| set_color.set("#f8fafc".to_string())></button>
                        <button class="ink-swatch" style="--swatch:#f87171;" on:click=move |_| set_color.set("#f87171".to_string())></button>
                        <button class="ink-swatch" style="--swatch:#34d399;" on:click=move |_| set_color.set("#34d399".to_string())></button>
                        <button class="ink-swatch" style="--swatch:#60a5fa;" on:click=move |_| set_color.set("#60a5fa".to_string())></button>
                        <button class="ink-swatch" style="--swatch:#f59e0b;" on:click=move |_| set_color.set("#f59e0b".to_string())></button>
                    </div>
                    <label class="ink-slider-label">
                        "Stroke width"
                        <input
                            type="range"
                            min="1"
                            max="24"
                            step="1"
                            prop:value=move || format!("{:.0}", stroke_width.get())
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                    set_stroke_width.set(v);
                                }
                            }
                        />
                    </label>
                    <label class="ink-slider-label">
                        "Opacity"
                        <input
                            type="range"
                            min="0.1"
                            max="1"
                            step="0.05"
                            prop:value=move || format!("{:.2}", opacity.get())
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                    set_opacity.set(v.clamp(0.1, 1.0));
                                }
                            }
                        />
                    </label>
                    <div class="ink-panel-title">"Assets"</div>
                    <button class="mode-btn ink-side-btn" on:click=on_add_image>"Add Image URL"</button>
                    <button class="mode-btn ink-side-btn" on:click=on_click_upload_image>"Upload Image"</button>
                    <div class="ink-panel-title">"Layers/Actions"</div>
                    <div class="ink-layer-strip">
                        <span>{move || format!("Strokes: {}", strokes.get().len())}</span>
                        <span>{move || format!("Images: {}", embeds.get().len())}</span>
                    </div>
                    <div class="ink-action-strip">
                        <button class="mode-btn ink-strip-btn" on:click=on_undo title="Undo (Cmd/Ctrl+Z)">"Undo"</button>
                        <button class="mode-btn ink-strip-btn" on:click=on_redo title="Redo (Cmd/Ctrl+Y)">"Redo"</button>
                        <button class="mode-btn ink-strip-btn" on:click=on_delete_selected title="Delete Selection (Delete)">"Delete"</button>
                        <button class="mode-btn ink-strip-btn" on:click=on_clear>"Clear"</button>
                    </div>
                    <button class="mode-btn ink-side-btn" on:click=on_delete_selected>"Delete Selected"</button>
                    <button class="mode-btn ink-side-btn" on:click=on_clear>"Clear Board"</button>
                    <button class="mode-btn ink-side-btn" on:click=on_reset_view>"Reset View"</button>
                </div>
                <button
                    class="mode-btn ink-panel-toggle"
                    on:click=move |_| set_panel_open.update(|open| *open = !*open)
                    aria-label="Toggle whiteboard controls"
                >
                    {move || if panel_open.get() { "Hide controls" } else { "Controls" }}
                </button>

                <div
                    class="ink-canvas-wrap"
                    style=move || format!("background:{};", background.get())
                    on:pointerdown=on_pointer_down
                    on:pointermove=on_pointer_move
                    on:pointerup=on_pointer_up
                    on:pointercancel=on_pointer_up
                >
                    <input
                        id=INK_IMAGE_UPLOAD_INPUT_ID
                        class="media-hidden-input"
                        type="file"
                        accept="image/*"
                        on:change=on_upload_image
                    />
                    <For
                        each=move || embeds.get()
                        key=|embed| embed.id.clone()
                        children=move |embed| {
                            let id_drag = embed.id.clone();
                            let id_resize = embed.id.clone();
                            let id_remove = embed.id.clone();
                            let id_style = embed.id.clone();
                            let id_selected = embed.id.clone();
                            let id_img_kind = embed.id.clone();
                            let id_vid_kind = embed.id.clone();
                            let id_img_src = embed.id.clone();
                            view! {
                                <div
                                    class="ink-embed-node"
                                    style=move || {
                                        if let Some(current) = embeds
                                            .get()
                                            .into_iter()
                                            .find(|item| item.id == id_style)
                                        {
                                            embed_style(
                                                &current,
                                                canvas_ref.get(),
                                                canvas_width as f64,
                                                canvas_height as f64,
                                                camera_x.get(),
                                                camera_y.get(),
                                                zoom.get(),
                                                selected_embed_id.get().as_deref() == Some(id_selected.as_str()),
                                            )
                                        } else {
                                            "display:none;".to_string()
                                        }
                                    }
                                    on:pointerdown=move |ev| on_embed_pointer_down(ev, id_drag.clone())
                                    on:pointermove=on_pointer_move
                                    on:pointerup=on_pointer_up
                                    on:pointercancel=on_pointer_up
                                >
                                    <button
                                        class="ink-embed-remove"
                                        on:pointerdown=move |ev| {
                                            ev.stop_propagation();
                                        }
                                        on:click=move |_| on_remove_embed(id_remove.clone())
                                    >
                                        "x"
                                    </button>
                                    <div
                                        class="ink-embed-resize"
                                        on:pointerdown=move |ev| on_embed_resize_pointer_down(ev, id_resize.clone())
                                        on:pointermove=on_pointer_move
                                        on:pointerup=on_pointer_up
                                        on:pointercancel=on_pointer_up
                                    ></div>
                                    <img
                                        class="ink-embed-media"
                                        style=move || {
                                            let wanted_id = id_img_kind.clone();
                                            let is_image = embeds
                                                .get()
                                                .into_iter()
                                                .find(|item| item.id == wanted_id)
                                                .map(|item| matches!(item.kind, InkEmbedKind::Image))
                                                .unwrap_or(false);
                                            if is_image { "display:block;" } else { "display:none;" }
                                        }
                                        src=move || {
                                            let wanted_id = id_img_src.clone();
                                            embeds
                                                .get()
                                                .into_iter()
                                                .find(|item| item.id == wanted_id)
                                                .map(|item| item.src)
                                                .unwrap_or_default()
                                        }
                                        alt="Whiteboard embed"
                                    />
                                    <div
                                        class="ink-embed-unsupported"
                                        style=move || {
                                            let wanted_id = id_vid_kind.clone();
                                            let is_video = embeds
                                                .get()
                                                .into_iter()
                                                .find(|item| item.id == wanted_id)
                                                .map(|item| matches!(item.kind, InkEmbedKind::Video))
                                                .unwrap_or(false);
                                            if is_video { "display:grid;" } else { "display:none;" }
                                        }
                                    >
                                        "Video embeds are no longer supported."
                                    </div>
                                </div>
                            }
                        }
                    />
                    <canvas
                        node_ref=canvas_ref
                        class="ink-canvas"
                        width=backing_canvas_width
                        height=backing_canvas_height
                        style=move || {
                            match tool.get() {
                                InkTool::Select => "pointer-events:none; z-index:4;".to_string(),
                                InkTool::Pinch => {
                                    if is_panning.get() {
                                        "pointer-events:auto; z-index:4; cursor:grabbing;".to_string()
                                    } else {
                                        "pointer-events:auto; z-index:4; cursor:grab;".to_string()
                                    }
                                }
                                _ => "pointer-events:auto; z-index:4; cursor:crosshair;".to_string(),
                            }
                        }
                        on:pointercancel=on_pointer_up
                        on:wheel=on_wheel
                    ></canvas>
                </div>
                <div class="ink-bottom-bar">
                    <button class="mode-btn ink-bottom-btn" on:click=on_zoom_out>"-"</button>
                    <span class="ink-bottom-zoom">{move || format!("{:.0}%", zoom.get() * 100.0)}</span>
                    <button class="mode-btn ink-bottom-btn" on:click=on_zoom_in>"+"</button>
                    <span class="ink-hint">"Mouse: Space+Drag or M tool pan, Wheel zoom | Keyboard: Tab/Shift+Tab move focus, Enter/Space activate, V/M/P/H/E/L/R/C tools"</span>
                </div>
            </div>
        </div>
    }
}

fn draw_scene(
    canvas: &HtmlCanvasElement,
    strokes: &[InkStroke],
    _embeds: &[InkEmbed],
    tool: InkTool,
    draft_points: &[InkPoint],
    shape_start: Option<InkPoint>,
    shape_end: Option<InkPoint>,
    selection_rect: Option<(f64, f64, f64, f64)>,
    selected_ids: &HashSet<String>,
    camera_x: f64,
    camera_y: f64,
    zoom: f64,
    pixel_ratio: f64,
) {
    let Some(ctx) = canvas_context(canvas) else {
        return;
    };
    let backing_width = canvas.width() as f64;
    let backing_height = canvas.height() as f64;
    let width = backing_width / pixel_ratio;
    let height = backing_height / pixel_ratio;
    ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0).ok();
    ctx.clear_rect(0.0, 0.0, backing_width, backing_height);
    ctx.set_transform(pixel_ratio, 0.0, 0.0, pixel_ratio, 0.0, 0.0)
        .ok();
    draw_grid(&ctx, width, height, camera_x, camera_y, zoom);

    let mut ordered_strokes = strokes.to_vec();
    ordered_strokes.sort_by_key(|stroke| stroke.z_index);
    for stroke in &ordered_strokes {
        draw_stroke(&ctx, stroke, width, height, camera_x, camera_y, zoom);
        if selected_ids.contains(&stroke.id) {
            let (x1, y1, x2, y2) = stroke_bounds(stroke);
            let (sx1, sy1) = world_to_screen(x1, y1, width, height, camera_x, camera_y, zoom);
            let (sx2, sy2) = world_to_screen(x2, y2, width, height, camera_x, camera_y, zoom);
            ctx.set_stroke_style_str("#22d3ee");
            ctx.set_line_width(1.5);
            ctx.set_line_dash(&js_sys::Array::of2(&4.into(), &4.into()))
                .ok();
            ctx.stroke_rect(
                sx1.min(sx2),
                sy1.min(sy2),
                (sx2 - sx1).abs(),
                (sy2 - sy1).abs(),
            );
            ctx.set_line_dash(&js_sys::Array::new()).ok();
        }
    }

    if matches!(tool, InkTool::Pen | InkTool::Highlighter) && draft_points.len() > 1 {
        let draft = InkStroke {
            id: "draft".to_string(),
            tool,
            color: "#cbd5e1".to_string(),
            width: 2.0,
            opacity: if matches!(tool, InkTool::Highlighter) {
                0.35
            } else {
                1.0
            },
            points: draft_points.to_vec(),
            z_index: i32::MAX,
        };
        draw_stroke(&ctx, &draft, width, height, camera_x, camera_y, zoom);
    }

    if let (Some(start), Some(end)) = (shape_start, shape_end)
        && matches!(tool, InkTool::Line | InkTool::Rectangle | InkTool::Circle)
    {
        let (sx, sy) = world_to_screen(start.x, start.y, width, height, camera_x, camera_y, zoom);
        let (ex, ey) = world_to_screen(end.x, end.y, width, height, camera_x, camera_y, zoom);
        ctx.set_stroke_style_str("#a7f3d0");
        ctx.set_line_width(2.0);
        ctx.begin_path();
        match tool {
            InkTool::Line => {
                ctx.move_to(sx, sy);
                ctx.line_to(ex, ey);
            }
            InkTool::Rectangle => {
                ctx.rect(sx.min(ex), sy.min(ey), (sx - ex).abs(), (sy - ey).abs())
            }
            InkTool::Circle => {
                let cx = (sx + ex) * 0.5;
                let cy = (sy + ey) * 0.5;
                let r = ((sx - ex).abs() * 0.5).max((sy - ey).abs() * 0.5).max(1.0);
                ctx.arc(cx, cy, r, 0.0, std::f64::consts::TAU).ok();
            }
            _ => {}
        }
        ctx.stroke();
    }

    if let Some((x1, y1, x2, y2)) = selection_rect {
        let (sx1, sy1) = world_to_screen(x1, y1, width, height, camera_x, camera_y, zoom);
        let (sx2, sy2) = world_to_screen(x2, y2, width, height, camera_x, camera_y, zoom);
        ctx.set_stroke_style_str("#60a5fa");
        ctx.set_line_width(1.0);
        ctx.set_line_dash(&js_sys::Array::of2(&6.into(), &3.into()))
            .ok();
        ctx.stroke_rect(
            sx1.min(sx2),
            sy1.min(sy2),
            (sx1 - sx2).abs(),
            (sy1 - sy2).abs(),
        );
        ctx.set_line_dash(&js_sys::Array::new()).ok();
    }
}

fn draw_stroke(
    ctx: &CanvasRenderingContext2d,
    stroke: &InkStroke,
    width: f64,
    height: f64,
    camera_x: f64,
    camera_y: f64,
    zoom: f64,
) {
    if stroke.points.is_empty() {
        return;
    }
    ctx.save();
    ctx.set_global_alpha(stroke.opacity.clamp(0.05, 1.0));
    ctx.set_stroke_style_str(&stroke.color);
    ctx.set_line_join("round");
    ctx.set_line_cap("round");

    match stroke.tool {
        InkTool::Line | InkTool::Rectangle | InkTool::Circle => {
            ctx.set_line_width(stroke_base_width(stroke, zoom));
            ctx.begin_path();
            if stroke.points.len() >= 2 {
                let a = stroke.points[0];
                let b = stroke.points[1];
                let (ax, ay) = world_to_screen(a.x, a.y, width, height, camera_x, camera_y, zoom);
                let (bx, by) = world_to_screen(b.x, b.y, width, height, camera_x, camera_y, zoom);
                match stroke.tool {
                    InkTool::Line => {
                        ctx.move_to(ax, ay);
                        ctx.line_to(bx, by);
                    }
                    InkTool::Rectangle => {
                        ctx.rect(ax.min(bx), ay.min(by), (ax - bx).abs(), (ay - by).abs());
                    }
                    InkTool::Circle => {
                        let cx = (ax + bx) * 0.5;
                        let cy = (ay + by) * 0.5;
                        let r = ((ax - bx).abs() * 0.5).max((ay - by).abs() * 0.5).max(1.0);
                        ctx.arc(cx, cy, r, 0.0, std::f64::consts::TAU).ok();
                    }
                    _ => {}
                }
            }
            ctx.stroke();
        }
        InkTool::Pen => {
            draw_pressure_sensitive_stroke(ctx, stroke, width, height, camera_x, camera_y, zoom);
        }
        InkTool::Highlighter => {
            draw_smooth_polyline(ctx, stroke, width, height, camera_x, camera_y, zoom);
        }
        _ => {
            draw_smooth_polyline(ctx, stroke, width, height, camera_x, camera_y, zoom);
        }
    }
    ctx.restore();
}

fn stroke_base_width(stroke: &InkStroke, zoom: f64) -> f64 {
    (stroke.width.max(1.0) * zoom).max(1.0)
}

fn pointer_pressure(ev: &PointerEvent) -> f64 {
    let raw = (ev.pressure() as f64).clamp(0.0, 1.0);
    match ev.pointer_type().as_str() {
        "pen" => raw.max(0.02),
        "touch" => {
            if raw > 0.0 {
                raw
            } else {
                0.55
            }
        }
        _ => 0.55,
    }
}

fn draw_smooth_polyline(
    ctx: &CanvasRenderingContext2d,
    stroke: &InkStroke,
    width: f64,
    height: f64,
    camera_x: f64,
    camera_y: f64,
    zoom: f64,
) {
    if stroke.points.is_empty() {
        return;
    }
    if stroke.points.len() == 1 {
        let p = stroke.points[0];
        let (x, y) = world_to_screen(p.x, p.y, width, height, camera_x, camera_y, zoom);
        let radius = stroke_base_width(stroke, zoom) * 0.5;
        ctx.set_fill_style_str(&stroke.color);
        ctx.begin_path();
        ctx.arc(x, y, radius, 0.0, std::f64::consts::TAU).ok();
        ctx.fill();
        return;
    }
    let screen_points = stroke
        .points
        .iter()
        .map(|p| world_to_screen(p.x, p.y, width, height, camera_x, camera_y, zoom))
        .collect::<Vec<_>>();
    ctx.set_line_width(stroke_base_width(stroke, zoom));
    ctx.begin_path();
    ctx.move_to(screen_points[0].0, screen_points[0].1);
    if screen_points.len() == 2 {
        let p = screen_points[1];
        ctx.line_to(p.0, p.1);
    } else {
        for i in 1..screen_points.len() - 1 {
            let current = screen_points[i];
            let next = screen_points[i + 1];
            let mid_x = (current.0 + next.0) * 0.5;
            let mid_y = (current.1 + next.1) * 0.5;
            ctx.quadratic_curve_to(current.0, current.1, mid_x, mid_y);
        }
        let last = screen_points[screen_points.len() - 1];
        ctx.line_to(last.0, last.1);
    }
    ctx.stroke();
}

fn infer_pressure_fallback(screen_points: &mut [(f64, f64, f64)]) {
    if screen_points.is_empty() {
        return;
    }
    let (min_pressure, max_pressure, sum_pressure) = screen_points.iter().fold(
        (1.0f64, 0.0f64, 0.0f64),
        |(min_p, max_p, sum_p), p| (min_p.min(p.2), max_p.max(p.2), sum_p + p.2),
    );
    let range = max_pressure - min_pressure;
    let avg = sum_pressure / screen_points.len() as f64;

    if range < 0.01 {
        if (avg - 0.5).abs() < 0.06 && screen_points.len() > 1 {
            // Mouse input usually reports a flat pressure around 0.5; simulate light dynamics from speed.
            let mut simulated = vec![0.72; screen_points.len()];
            for i in 0..screen_points.len() {
                let prev = if i == 0 {
                    screen_points[i]
                } else {
                    screen_points[i - 1]
                };
                let next = if i + 1 >= screen_points.len() {
                    screen_points[i]
                } else {
                    screen_points[i + 1]
                };
                let speed = ((next.0 - prev.0).powi(2) + (next.1 - prev.1).powi(2)).sqrt();
                let pressure = (1.0 - (speed / 7.5).clamp(0.0, 1.0)).powf(0.65);
                simulated[i] = (0.28 + pressure * 0.64).clamp(0.18, 0.95);
            }
            for i in 1..simulated.len() {
                simulated[i] = simulated[i - 1] * 0.25 + simulated[i] * 0.75;
            }
            for i in (0..simulated.len() - 1).rev() {
                simulated[i] = simulated[i + 1] * 0.2 + simulated[i] * 0.8;
            }
            for (idx, point) in screen_points.iter_mut().enumerate() {
                point.2 = simulated[idx];
            }
            return;
        }

        let fallback = if (avg - 0.5).abs() < 0.06 {
            0.72
        } else {
            avg.clamp(0.08, 1.0)
        };
        for p in screen_points.iter_mut() {
            p.2 = fallback;
        }
        return;
    }

    if range < 0.08 {
        for p in screen_points.iter_mut() {
            let expanded = (p.2 - min_pressure) / range.max(0.0001);
            let expanded = 0.1 + expanded * 0.9;
            p.2 = (p.2 * 0.4 + expanded * 0.6).clamp(0.05, 1.0);
        }
        return;
    }

    for p in screen_points.iter_mut() {
        p.2 = p.2.clamp(0.05, 1.0);
    }
}

fn pressure_easing(pressure: f64) -> f64 {
    pressure.clamp(0.0, 1.0).powf(0.62)
}

fn fill_smooth_closed_path(ctx: &CanvasRenderingContext2d, outline: &[(f64, f64)]) {
    if outline.len() < 3 {
        return;
    }

    let mut cleaned: Vec<(f64, f64)> = Vec::with_capacity(outline.len());
    for point in outline.iter().copied() {
        let should_push = cleaned.last().is_none_or(|last| {
            let dx = point.0 - last.0;
            let dy = point.1 - last.1;
            (dx * dx + dy * dy) > 0.0001
        });
        if should_push {
            cleaned.push(point);
        }
    }
    if cleaned.len() < 3 {
        return;
    }

    let start_mid = (
        (cleaned[cleaned.len() - 1].0 + cleaned[0].0) * 0.5,
        (cleaned[cleaned.len() - 1].1 + cleaned[0].1) * 0.5,
    );
    ctx.begin_path();
    ctx.move_to(start_mid.0, start_mid.1);
    for idx in 0..cleaned.len() {
        let current = cleaned[idx];
        let next = cleaned[(idx + 1) % cleaned.len()];
        let mid = ((current.0 + next.0) * 0.5, (current.1 + next.1) * 0.5);
        ctx.quadratic_curve_to(current.0, current.1, mid.0, mid.1);
    }
    ctx.close_path();
    ctx.fill();
}

fn draw_pressure_outline(
    ctx: &CanvasRenderingContext2d,
    color: &str,
    points: &[(f64, f64, f64)],
    base_width: f64,
) {
    if points.is_empty() {
        return;
    }

    let mut working = points.to_vec();
    infer_pressure_fallback(&mut working);
    if working.is_empty() {
        return;
    }
    let freehand_points = working
        .iter()
        .map(|point| InputPoint::Struct {
            x: point.0,
            y: point.1,
            pressure: Some(point.2.clamp(0.0, 1.0)),
        })
        .collect::<Vec<_>>();
    let options = StrokeOptions {
        size: Some(base_width.max(0.9)),
        thinning: Some(0.68),
        smoothing: Some(0.78),
        streamline: Some(0.64),
        easing: Some(pressure_easing),
        simulate_pressure: Some(false),
        start: None,
        end: None,
        last: Some(true),
        closed: Some(false),
    };
    let outline = get_stroke(&freehand_points, &options)
        .into_iter()
        .map(|point| (point[0], point[1]))
        .collect::<Vec<_>>();
    if outline.len() < 3 && let Some(point) = working.first() {
        ctx.set_fill_style_str(color);
        ctx.begin_path();
        ctx.arc(
            point.0,
            point.1,
            (base_width * 0.5).max(0.8),
            0.0,
            std::f64::consts::TAU,
        )
        .ok();
        ctx.fill();
        return;
    }

    ctx.set_fill_style_str(color);
    fill_smooth_closed_path(ctx, &outline);
}

fn draw_pressure_sensitive_stroke(
    ctx: &CanvasRenderingContext2d,
    stroke: &InkStroke,
    width: f64,
    height: f64,
    camera_x: f64,
    camera_y: f64,
    zoom: f64,
) {
    let screen_points = stroke
        .points
        .iter()
        .map(|point| {
            let (sx, sy) = world_to_screen(point.x, point.y, width, height, camera_x, camera_y, zoom);
            (sx, sy, point.pressure.clamp(0.0, 1.0))
        })
        .collect::<Vec<_>>();
    draw_pressure_outline(
        ctx,
        &stroke.color,
        &screen_points,
        stroke_base_width(stroke, zoom),
    );
}

fn draw_grid(
    ctx: &CanvasRenderingContext2d,
    width: f64,
    height: f64,
    camera_x: f64,
    camera_y: f64,
    zoom: f64,
) {
    let spacing_world = 100.0;
    let spacing_screen = spacing_world * zoom;
    if spacing_screen < 18.0 {
        return;
    }
    ctx.save();
    ctx.set_stroke_style_str("rgba(148, 163, 184, 0.16)");
    ctx.set_line_width(1.0);
    let origin_x = width * 0.5 - camera_x * zoom;
    let origin_y = height * 0.5 - camera_y * zoom;

    let mut x = origin_x.rem_euclid(spacing_screen);
    while x < width {
        ctx.begin_path();
        ctx.move_to(x, 0.0);
        ctx.line_to(x, height);
        ctx.stroke();
        x += spacing_screen;
    }

    let mut y = origin_y.rem_euclid(spacing_screen);
    while y < height {
        ctx.begin_path();
        ctx.move_to(0.0, y);
        ctx.line_to(width, y);
        ctx.stroke();
        y += spacing_screen;
    }
    ctx.restore();
}

fn embed_style(
    embed: &InkEmbed,
    canvas: Option<HtmlCanvasElement>,
    canvas_width: f64,
    canvas_height: f64,
    camera_x: f64,
    camera_y: f64,
    zoom: f64,
    is_selected: bool,
) -> String {
    let (sx, sy) = world_to_screen(
        embed.x,
        embed.y,
        canvas_width,
        canvas_height,
        camera_x,
        camera_y,
        zoom,
    );
    let (scale_x, scale_y) = if let Some(canvas) = canvas {
        let rect = canvas.get_bounding_client_rect();
        let x = if canvas_width > 0.0 {
            rect.width() / canvas_width
        } else {
            1.0
        };
        let y = if canvas_height > 0.0 {
            rect.height() / canvas_height
        } else {
            1.0
        };
        (x, y)
    } else {
        (1.0, 1.0)
    };
    let left = sx * scale_x;
    let top = sy * scale_y;
    let border_color = if is_selected {
        "color-mix(in srgb, #a78bfa, transparent 8%)"
    } else {
        "color-mix(in srgb, #7dd3fc, transparent 38%)"
    };
    let ring = if is_selected {
        "0 0 0 1px rgba(167,139,250,0.7), 0 8px 20px rgba(2, 6, 23, 0.35)"
    } else {
        "0 8px 20px rgba(2, 6, 23, 0.35)"
    };
    format!(
        "left: {left}px; top: {top}px; width: {}px; height: {}px; z-index:{}; border-color:{border_color}; box-shadow:{ring};",
        ((embed.width * zoom) * scale_x).max(36.0),
        ((embed.height * zoom) * scale_y).max(28.0),
        2 + embed.z_index
    )
}

fn world_to_screen(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    camera_x: f64,
    camera_y: f64,
    zoom: f64,
) -> (f64, f64) {
    (
        (x - camera_x) * zoom + width * 0.5,
        (y - camera_y) * zoom + height * 0.5,
    )
}

fn screen_to_world(
    sx: f64,
    sy: f64,
    width: f64,
    height: f64,
    camera_x: f64,
    camera_y: f64,
    zoom: f64,
) -> (f64, f64) {
    (
        (sx - width * 0.5) / zoom + camera_x,
        (sy - height * 0.5) / zoom + camera_y,
    )
}

fn canvas_context(canvas: &HtmlCanvasElement) -> Option<CanvasRenderingContext2d> {
    canvas
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|ctx| ctx.dyn_into::<CanvasRenderingContext2d>().ok())
}

fn stroke_bounds(stroke: &InkStroke) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for point in &stroke.points {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }
    if !min_x.is_finite() {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        (min_x, min_y, max_x, max_y)
    }
}

fn stroke_hit_test(stroke: &InkStroke, point: InkPoint, zoom: f64) -> bool {
    let radius = (18.0 / zoom.max(0.25)).max(stroke.width * 0.65 + 6.0);
    let radius_sq = radius * radius;
    stroke.points.iter().any(|p| {
        let dx = p.x - point.x;
        let dy = p.y - point.y;
        dx * dx + dy * dy <= radius_sq
    })
}

fn next_stroke_z(strokes: &[InkStroke]) -> i32 {
    strokes.iter().map(|stroke| stroke.z_index).max().unwrap_or(0) + 1
}

fn pick_stroke_at_point(strokes: &[InkStroke], point: InkPoint, zoom: f64) -> Option<String> {
    let mut ordered = strokes.to_vec();
    ordered.sort_by_key(|stroke| stroke.z_index);
    ordered
        .into_iter()
        .rev()
        .find(|stroke| stroke_hit_test(stroke, point, zoom))
        .map(|stroke| stroke.id)
}

fn render_thumbnail_data_url(strokes: &[InkStroke], embeds: &[InkEmbed], background: &str) -> Option<String> {
    let (min_x, min_y, max_x, max_y) = all_content_bounds(strokes, embeds)?;
    let bounds_w = (max_x - min_x).max(1.0);
    let bounds_h = (max_y - min_y).max(1.0);
    let aspect = bounds_w / bounds_h;

    let (canvas_w, canvas_h) = if aspect >= 1.0 {
        (1400.0, (1400.0 / aspect).clamp(380.0, 980.0))
    } else {
        ((1100.0 * aspect).clamp(420.0, 1100.0), 1100.0)
    };

    let document = window()?.document()?;
    let canvas = document
        .create_element("canvas")
        .ok()?
        .dyn_into::<HtmlCanvasElement>()
        .ok()?;
    canvas.set_width(canvas_w as u32);
    canvas.set_height(canvas_h as u32);
    let ctx = canvas_context(&canvas)?;
    ctx.set_fill_style_str(&sanitize_hex_color(background));
    ctx.fill_rect(0.0, 0.0, canvas_w, canvas_h);

    let margin = 32.0;
    let sx = (canvas_w - margin * 2.0) / bounds_w;
    let sy = (canvas_h - margin * 2.0) / bounds_h;
    let scale = sx.min(sy).max(0.0001);
    let offset_x = (canvas_w - bounds_w * scale) * 0.5;
    let offset_y = (canvas_h - bounds_h * scale) * 0.5;

    for embed in embeds {
        draw_embed_thumbnail(&ctx, embed, min_x, min_y, scale, offset_x, offset_y);
    }
    for stroke in strokes {
        draw_stroke_thumbnail(&ctx, stroke, min_x, min_y, scale, offset_x, offset_y);
    }

    canvas.to_data_url_with_type("image/png").ok()
}

fn draw_stroke_thumbnail(
    ctx: &CanvasRenderingContext2d,
    stroke: &InkStroke,
    min_x: f64,
    min_y: f64,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
) {
    if stroke.points.is_empty() {
        return;
    }
    let map_xy = |p: InkPoint| -> (f64, f64) {
        (
            (p.x - min_x) * scale + offset_x,
            (p.y - min_y) * scale + offset_y,
        )
    };

    ctx.save();
    ctx.set_global_alpha(stroke.opacity.clamp(0.05, 1.0));
    ctx.set_line_join("round");
    ctx.set_line_cap("round");

    match stroke.tool {
        InkTool::Pen => {
            let points = stroke
                .points
                .iter()
                .map(|point| {
                    let (x, y) = map_xy(*point);
                    (x, y, point.pressure.clamp(0.0, 1.0))
                })
                .collect::<Vec<_>>();
            let base_width = (stroke.width * scale).clamp(1.0, 14.0);
            draw_pressure_outline(ctx, &stroke.color, &points, base_width);
        }
        InkTool::Line | InkTool::Rectangle | InkTool::Circle => {
            ctx.set_stroke_style_str(&stroke.color);
            ctx.set_line_width((stroke.width * scale).clamp(1.0, 14.0));
            ctx.begin_path();
            if stroke.points.len() >= 2 {
                let a = stroke.points[0];
                let b = stroke.points[1];
                let (ax, ay) = map_xy(a);
                let (bx, by) = map_xy(b);
                match stroke.tool {
                    InkTool::Line => {
                        ctx.move_to(ax, ay);
                        ctx.line_to(bx, by);
                    }
                    InkTool::Rectangle => {
                        ctx.rect(ax.min(bx), ay.min(by), (ax - bx).abs(), (ay - by).abs());
                    }
                    InkTool::Circle => {
                        let cx = (ax + bx) * 0.5;
                        let cy = (ay + by) * 0.5;
                        let r = ((ax - bx).abs() * 0.5).max((ay - by).abs() * 0.5).max(1.0);
                        ctx.arc(cx, cy, r, 0.0, std::f64::consts::TAU).ok();
                    }
                    _ => {}
                }
            }
            ctx.stroke();
        }
        _ => {
            let screen_points = stroke
                .points
                .iter()
                .map(|point| map_xy(*point))
                .collect::<Vec<_>>();
            if screen_points.len() == 1 {
                let radius = (stroke.width * scale * 0.5).clamp(0.8, 7.0);
                ctx.set_fill_style_str(&stroke.color);
                ctx.begin_path();
                ctx.arc(
                    screen_points[0].0,
                    screen_points[0].1,
                    radius,
                    0.0,
                    std::f64::consts::TAU,
                )
                .ok();
                ctx.fill();
            } else {
                ctx.set_stroke_style_str(&stroke.color);
                ctx.set_line_width((stroke.width * scale).clamp(1.0, 14.0));
                ctx.begin_path();
                ctx.move_to(screen_points[0].0, screen_points[0].1);
                for i in 1..screen_points.len() - 1 {
                    let current = screen_points[i];
                    let next = screen_points[i + 1];
                    let mid_x = (current.0 + next.0) * 0.5;
                    let mid_y = (current.1 + next.1) * 0.5;
                    ctx.quadratic_curve_to(current.0, current.1, mid_x, mid_y);
                }
                let last = screen_points[screen_points.len() - 1];
                ctx.line_to(last.0, last.1);
                ctx.stroke();
            }
        }
    }
    ctx.restore();
}

fn draw_embed_thumbnail(
    ctx: &CanvasRenderingContext2d,
    embed: &InkEmbed,
    min_x: f64,
    min_y: f64,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
) {
    let x = (embed.x - min_x) * scale + offset_x;
    let y = (embed.y - min_y) * scale + offset_y;
    let w = (embed.width * scale).max(4.0);
    let h = (embed.height * scale).max(4.0);

    if matches!(embed.kind, InkEmbedKind::Image)
        && embed.src.starts_with("data:image/")
        && let Some(img) = find_loaded_embed_image(&embed.src)
        && ctx
            .draw_image_with_html_image_element_and_dw_and_dh(&img, x, y, w, h)
            .is_ok()
    {
        ctx.save();
        ctx.set_stroke_style_str("rgba(148, 163, 184, 0.45)");
        ctx.set_line_width(1.0);
        ctx.stroke_rect(x, y, w, h);
        ctx.restore();
        return;
    }

    ctx.save();
    ctx.set_fill_style_str("rgba(30, 41, 59, 0.72)");
    ctx.fill_rect(x, y, w, h);
    ctx.set_stroke_style_str("rgba(94, 234, 212, 0.9)");
    ctx.set_line_width(1.0);
    ctx.stroke_rect(x, y, w, h);
    ctx.set_fill_style_str("rgba(226, 232, 240, 0.92)");
    ctx.set_font("12px ui-sans-serif");
    let label = if matches!(embed.kind, InkEmbedKind::Video) {
        "Video"
    } else {
        "Image"
    };
    let _ = ctx.fill_text(label, x + 6.0, y + 16.0);
    ctx.restore();
}

fn find_loaded_embed_image(src: &str) -> Option<HtmlImageElement> {
    let document = window()?.document()?;
    let nodes = document.get_elements_by_class_name("ink-embed-media");
    let mut idx = 0;
    while idx < nodes.length() {
        if let Some(node) = nodes.item(idx)
            && let Ok(img) = node.dyn_into::<HtmlImageElement>()
        {
            let candidate = img.get_attribute("src").unwrap_or_default();
            if candidate == src {
                return Some(img);
            }
        }
        idx += 1;
    }
    None
}

fn all_content_bounds(strokes: &[InkStroke], embeds: &[InkEmbed]) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for stroke in strokes {
        for point in &stroke.points {
            min_x = min_x.min(point.x);
            min_y = min_y.min(point.y);
            max_x = max_x.max(point.x);
            max_y = max_y.max(point.y);
        }
    }
    for embed in embeds {
        min_x = min_x.min(embed.x);
        min_y = min_y.min(embed.y);
        max_x = max_x.max(embed.x + embed.width);
        max_y = max_y.max(embed.y + embed.height);
    }

    if !min_x.is_finite() {
        None
    } else {
        Some((min_x, min_y, max_x, max_y))
    }
}

fn sanitize_hex_color(value: &str) -> String {
    let trimmed = value.trim();
    let is_hex = trimmed.len() == 7
        && trimmed.starts_with('#')
        && trimmed
            .chars()
            .skip(1)
            .all(|ch| ch.is_ascii_hexdigit());
    if is_hex {
        trimmed.to_string()
    } else {
        "#0b1020".to_string()
    }
}

fn sanitize_ink_name(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "Whiteboard".to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}

fn keyboard_event_targets_text_input(ev: &KeyboardEvent) -> bool {
    ev.target()
        .and_then(|target| target.dyn_into::<Element>().ok())
        .map(|el| {
            let tag = el.tag_name().to_ascii_lowercase();
            let editable = el
                .get_attribute("contenteditable")
                .map(|value| !value.eq_ignore_ascii_case("false"))
                .unwrap_or(false);
            tag == "input" || tag == "textarea" || editable
        })
        .unwrap_or(false)
}
