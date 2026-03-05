use crate::app::bindings::download_data_url;
use crate::models::{InkDocument, InkPoint, InkStroke, InkTool};
use leptos::ev::{KeyboardEvent, PointerEvent, WheelEvent};
use leptos::html::Canvas;
use leptos::prelude::*;
use std::collections::HashSet;
use uuid::Uuid;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, window};

const MIN_ZOOM: f64 = 0.2;
const MAX_ZOOM: f64 = 4.0;

#[component]
pub fn InkCanvasModal(
    initial_document: InkDocument,
    on_cancel: Callback<()>,
    on_save: Callback<InkDocument>,
) -> impl IntoView {
    let (tool, set_tool) = signal(InkTool::Pen);
    let (color, set_color) = signal("#e5e7eb".to_string());
    let (stroke_width, set_stroke_width) = signal(3.0f64);
    let (opacity, set_opacity) = signal(1.0f64);
    let (strokes, set_strokes) = signal(initial_document.strokes.clone());
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

    let canvas_ref = NodeRef::<Canvas>::new();
    let canvas_width = initial_document.width.max(1600.0) as u32;
    let canvas_height = initial_document.height.max(900.0) as u32;

    let to_world_point = move |ev: PointerEvent| -> Option<InkPoint> {
        let canvas = canvas_ref.get()?;
        let rect = canvas.get_bounding_client_rect();
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return None;
        }
        let sx = (ev.client_x() as f64 - rect.left()) * canvas.width() as f64 / rect.width();
        let sy = (ev.client_y() as f64 - rect.top()) * canvas.height() as f64 / rect.height();
        let (wx, wy) = screen_to_world(
            sx,
            sy,
            canvas.width() as f64,
            canvas.height() as f64,
            camera_x.get_untracked(),
            camera_y.get_untracked(),
            zoom.get_untracked(),
        );
        Some(InkPoint {
            x: wx,
            y: wy,
            pressure: ev.pressure().max(0.1) as f64,
        })
    };

    let redraw = move || {
        let Some(canvas) = canvas_ref.get() else {
            return;
        };
        draw_scene(
            &canvas,
            &strokes.get(),
            tool.get(),
            &draft_points.get(),
            shape_start.get(),
            shape_end.get(),
            selection_rect.get(),
            &selected_ids.get(),
            camera_x.get(),
            camera_y.get(),
            zoom.get(),
        );
    };

    Effect::new(move |_| {
        let _ = strokes.get();
        let _ = draft_points.get();
        let _ = shape_start.get();
        let _ = shape_end.get();
        let _ = selection_rect.get();
        let _ = selected_ids.get();
        let _ = camera_x.get();
        let _ = camera_y.get();
        let _ = zoom.get();
        redraw();
    });

    let erase_at = move |point: InkPoint| {
        let mut changed = false;
        set_strokes.update(|all| {
            let before = all.len();
            all.retain(|stroke| !stroke_hit_test(stroke, point));
            changed = before != all.len();
        });
        if changed {
            set_redo_stack.set(Vec::new());
        }
    };

    let commit_stroke = move |stroke: InkStroke| {
        set_strokes.update(|all| all.push(stroke));
        set_redo_stack.set(Vec::new());
    };

    let on_pointer_down = move |ev: PointerEvent| {
        ev.prevent_default();
        let button = ev.button();
        let pan_mode = button == 1 || (button == 0 && space_pressed.get_untracked());
        if pan_mode {
            set_is_panning.set(true);
            set_last_pan_client.set(Some((ev.client_x() as f64, ev.client_y() as f64)));
            return;
        }

        let Some(point) = to_world_point(ev) else {
            return;
        };
        set_is_drawing.set(true);
        match tool.get_untracked() {
            InkTool::Pen | InkTool::Highlighter => set_draft_points.set(vec![point]),
            InkTool::Eraser => erase_at(point),
            InkTool::Line | InkTool::Rectangle | InkTool::Circle => {
                set_shape_start.set(Some(point));
                set_shape_end.set(Some(point));
            }
            InkTool::Lasso | InkTool::Select => {
                set_selection_start.set(Some(point));
                set_selection_rect.set(Some((point.x, point.y, point.x, point.y)));
            }
        }
    };

    let on_pointer_move = move |ev: PointerEvent| {
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
        let Some(point) = to_world_point(ev) else {
            return;
        };
        match tool.get_untracked() {
            InkTool::Pen | InkTool::Highlighter => {
                set_draft_points.update(|points| points.push(point))
            }
            InkTool::Eraser => erase_at(point),
            InkTool::Line | InkTool::Rectangle | InkTool::Circle => set_shape_end.set(Some(point)),
            InkTool::Lasso | InkTool::Select => {
                if let Some(start) = selection_start.get_untracked() {
                    set_selection_rect.set(Some((start.x, start.y, point.x, point.y)));
                }
            }
        }
    };

    let on_pointer_up = move |_| {
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
                    });
                }
                set_shape_start.set(None);
                set_shape_end.set(None);
            }
            InkTool::Lasso | InkTool::Select => {
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
            InkTool::Eraser => {}
        }
    };

    let on_wheel = move |ev: WheelEvent| {
        ev.prevent_default();
        let Some(canvas) = canvas_ref.get() else {
            return;
        };
        let rect = canvas.get_bounding_client_rect();
        let sx = (ev.client_x() as f64 - rect.left()) * canvas.width() as f64 / rect.width();
        let sy = (ev.client_y() as f64 - rect.top()) * canvas.height() as f64 / rect.height();
        let old_zoom = zoom.get_untracked();
        let step = if ev.delta_y() < 0.0 { 1.12 } else { 0.89 };
        let new_zoom = (old_zoom * step).clamp(MIN_ZOOM, MAX_ZOOM);
        let (before_x, before_y) = screen_to_world(
            sx,
            sy,
            canvas.width() as f64,
            canvas.height() as f64,
            camera_x.get_untracked(),
            camera_y.get_untracked(),
            old_zoom,
        );
        set_zoom.set(new_zoom);
        let cam_x = before_x - (sx - canvas.width() as f64 * 0.5) / new_zoom;
        let cam_y = before_y - (sy - canvas.height() as f64 * 0.5) / new_zoom;
        set_camera_x.set(cam_x);
        set_camera_y.set(cam_y);
    };

    let on_key_down = move |ev: KeyboardEvent| {
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
    };

    let on_reset_view = move |_| {
        set_zoom.set(1.0);
        set_camera_x.set(0.0);
        set_camera_y.set(0.0);
    };

    let on_export_png = move |_| {
        let Some(canvas) = canvas_ref.get() else {
            return;
        };
        if let Ok(url) = canvas.to_data_url_with_type("image/png") {
            download_data_url("ink-drawing.png", &url);
        }
    };

    let initial_document_for_save = initial_document.clone();
    let on_save_click = move |_| {
        let mut doc = initial_document_for_save.clone();
        doc.strokes = strokes.get_untracked();
        doc.width = canvas_width as f64;
        doc.height = canvas_height as f64;
        let thumbnail = render_thumbnail_data_url(&doc.strokes).or_else(|| {
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
                <div class="ink-modal-header">
                    <h3>"Infinite Whiteboard (Excalidraw-style)"</h3>
                    <div class="ink-modal-actions">
                        <span class="ink-zoom-badge">{move || format!("{:.0}%", zoom.get() * 100.0)}</span>
                        <button class="mode-btn" on:click=on_undo>"Undo"</button>
                        <button class="mode-btn" on:click=on_redo>"Redo"</button>
                        <button class="mode-btn" on:click=on_reset_view>"Reset View"</button>
                        <button class="mode-btn" on:click=on_export_png>"Export PNG"</button>
                        <button class="mode-btn" on:click=move |_| on_cancel.run(())>"Close"</button>
                        <button class="mode-btn active" on:click=on_save_click>"Save"</button>
                    </div>
                </div>

                <div class="ink-toolbar">
                    <button class=move || if tool.get() == InkTool::Pen { "mode-btn active" } else { "mode-btn" } on:click=move |_| set_tool.set(InkTool::Pen)>"Pen"</button>
                    <button class=move || if tool.get() == InkTool::Highlighter { "mode-btn active" } else { "mode-btn" } on:click=move |_| set_tool.set(InkTool::Highlighter)>"Highlighter"</button>
                    <button class=move || if tool.get() == InkTool::Eraser { "mode-btn active" } else { "mode-btn" } on:click=move |_| set_tool.set(InkTool::Eraser)>"Eraser"</button>
                    <button class=move || if tool.get() == InkTool::Line { "mode-btn active" } else { "mode-btn" } on:click=move |_| set_tool.set(InkTool::Line)>"Line"</button>
                    <button class=move || if tool.get() == InkTool::Rectangle { "mode-btn active" } else { "mode-btn" } on:click=move |_| set_tool.set(InkTool::Rectangle)>"Rect"</button>
                    <button class=move || if tool.get() == InkTool::Circle { "mode-btn active" } else { "mode-btn" } on:click=move |_| set_tool.set(InkTool::Circle)>"Circle"</button>
                    <button class=move || if tool.get() == InkTool::Lasso { "mode-btn active" } else { "mode-btn" } on:click=move |_| set_tool.set(InkTool::Lasso)>"Lasso"</button>
                    <button class=move || if tool.get() == InkTool::Select { "mode-btn active" } else { "mode-btn" } on:click=move |_| set_tool.set(InkTool::Select)>"Select"</button>
                    <button class="mode-btn" on:click=on_delete_selected>"Delete Selected"</button>
                    <button class="mode-btn" on:click=on_clear>"Clear"</button>
                    <span class="ink-hint">"Space + Drag or Middle Mouse = Pan, Wheel = Zoom"</span>

                    <input
                        class="ink-color"
                        type="color"
                        prop:value=move || color.get()
                        on:input=move |ev| set_color.set(event_target_value(&ev))
                    />
                    <label class="ink-slider-label">
                        "Size"
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
                </div>

                <div class="ink-canvas-wrap">
                    <canvas
                        node_ref=canvas_ref
                        class="ink-canvas"
                        width=canvas_width
                        height=canvas_height
                        on:pointerdown=on_pointer_down
                        on:pointermove=on_pointer_move
                        on:pointerup=on_pointer_up
                        on:pointerleave=on_pointer_up
                        on:wheel=on_wheel
                    ></canvas>
                </div>
            </div>
        </div>
    }
}

fn draw_scene(
    canvas: &HtmlCanvasElement,
    strokes: &[InkStroke],
    tool: InkTool,
    draft_points: &[InkPoint],
    shape_start: Option<InkPoint>,
    shape_end: Option<InkPoint>,
    selection_rect: Option<(f64, f64, f64, f64)>,
    selected_ids: &HashSet<String>,
    camera_x: f64,
    camera_y: f64,
    zoom: f64,
) {
    let Some(ctx) = canvas_context(canvas) else {
        return;
    };
    let width = canvas.width() as f64;
    let height = canvas.height() as f64;
    ctx.set_fill_style_str("#111827");
    ctx.fill_rect(0.0, 0.0, width, height);
    draw_grid(&ctx, width, height, camera_x, camera_y, zoom);

    for stroke in strokes {
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
    ctx.set_line_width((stroke.width.max(1.0) * zoom).max(1.0));
    ctx.set_line_join("round");
    ctx.set_line_cap("round");
    ctx.begin_path();

    match stroke.tool {
        InkTool::Line | InkTool::Rectangle | InkTool::Circle => {
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
        }
        _ => {
            let first = stroke.points[0];
            let (fx, fy) =
                world_to_screen(first.x, first.y, width, height, camera_x, camera_y, zoom);
            ctx.move_to(fx, fy);
            for point in stroke.points.iter().skip(1) {
                let (px, py) =
                    world_to_screen(point.x, point.y, width, height, camera_x, camera_y, zoom);
                ctx.line_to(px, py);
            }
        }
    }
    ctx.stroke();
    ctx.restore();
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

fn stroke_hit_test(stroke: &InkStroke, point: InkPoint) -> bool {
    let radius = (stroke.width + 12.0).max(10.0);
    let radius_sq = radius * radius;
    stroke.points.iter().any(|p| {
        let dx = p.x - point.x;
        let dy = p.y - point.y;
        dx * dx + dy * dy <= radius_sq
    })
}

fn render_thumbnail_data_url(strokes: &[InkStroke]) -> Option<String> {
    let (min_x, min_y, max_x, max_y) = all_strokes_bounds(strokes)?;
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
    ctx.set_fill_style_str("#111827");
    ctx.fill_rect(0.0, 0.0, canvas_w, canvas_h);

    let margin = 32.0;
    let sx = (canvas_w - margin * 2.0) / bounds_w;
    let sy = (canvas_h - margin * 2.0) / bounds_h;
    let scale = sx.min(sy).max(0.0001);
    let offset_x = (canvas_w - bounds_w * scale) * 0.5;
    let offset_y = (canvas_h - bounds_h * scale) * 0.5;

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
    let map = |p: InkPoint| -> (f64, f64) {
        (
            (p.x - min_x) * scale + offset_x,
            (p.y - min_y) * scale + offset_y,
        )
    };

    ctx.save();
    ctx.set_global_alpha(stroke.opacity.clamp(0.05, 1.0));
    ctx.set_stroke_style_str(&stroke.color);
    ctx.set_line_width((stroke.width * scale).clamp(1.0, 14.0));
    ctx.set_line_join("round");
    ctx.set_line_cap("round");
    ctx.begin_path();

    match stroke.tool {
        InkTool::Line | InkTool::Rectangle | InkTool::Circle => {
            if stroke.points.len() >= 2 {
                let a = stroke.points[0];
                let b = stroke.points[1];
                let (ax, ay) = map(a);
                let (bx, by) = map(b);
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
        }
        _ => {
            let first = stroke.points[0];
            let (fx, fy) = map(first);
            ctx.move_to(fx, fy);
            for point in stroke.points.iter().skip(1) {
                let (px, py) = map(*point);
                ctx.line_to(px, py);
            }
        }
    }
    ctx.stroke();
    ctx.restore();
}

fn all_strokes_bounds(strokes: &[InkStroke]) -> Option<(f64, f64, f64, f64)> {
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

    if !min_x.is_finite() {
        None
    } else {
        Some((min_x, min_y, max_x, max_y))
    }
}
