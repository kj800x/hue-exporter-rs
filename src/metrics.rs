use crate::hue::{BridgeState, LightState};

/// Render the current state as Prometheus text exposition format.
pub fn render(state: &Option<BridgeState>) -> String {
    let mut out = String::with_capacity(4096);

    out.push_str("# HELP hue_bridge_up Whether the Hue bridge is reachable\n");
    out.push_str("# TYPE hue_bridge_up gauge\n");

    let Some(state) = state else {
        // Loss of signal — only report bridge down, nothing else.
        out.push_str("hue_bridge_up 0\n");
        return out;
    };

    out.push_str("hue_bridge_up 1\n\n");

    write_light_metrics(&mut out, &state.lights);
    out
}

fn write_light_metrics(out: &mut String, lights: &[LightState]) {
    if lights.is_empty() {
        return;
    }

    // reachable — always reported for every known light
    write_header(
        out,
        "hue_light_reachable",
        "Whether the light is reachable from the bridge",
        "gauge",
    );
    for l in lights {
        write_gauge(
            out,
            "hue_light_reachable",
            l,
            if l.reachable { 1.0 } else { 0.0 },
        );
    }
    out.push('\n');

    // on
    write_header(
        out,
        "hue_light_on",
        "Whether the light is turned on",
        "gauge",
    );
    for l in lights {
        if let Some(on) = l.on {
            write_gauge(out, "hue_light_on", l, if on { 1.0 } else { 0.0 });
        }
    }
    out.push('\n');

    // brightness
    write_header(
        out,
        "hue_light_brightness",
        "Light brightness percentage (0-100)",
        "gauge",
    );
    for l in lights {
        if let Some(b) = l.brightness {
            write_gauge(out, "hue_light_brightness", l, b);
        }
    }
    out.push('\n');

    // color xy
    write_header(
        out,
        "hue_light_color_xy_x",
        "CIE x color coordinate",
        "gauge",
    );
    for l in lights {
        if let Some((x, _)) = l.color_xy {
            write_gauge(out, "hue_light_color_xy_x", l, x);
        }
    }
    out.push('\n');

    write_header(
        out,
        "hue_light_color_xy_y",
        "CIE y color coordinate",
        "gauge",
    );
    for l in lights {
        if let Some((_, y)) = l.color_xy {
            write_gauge(out, "hue_light_color_xy_y", l, y);
        }
    }
    out.push('\n');

    // color hue and saturation (derived from CIE xy + brightness)
    write_header(
        out,
        "hue_light_color_hue",
        "Color hue in degrees (0-360)",
        "gauge",
    );
    for l in lights {
        if let Some(h) = l.color_hue {
            write_gauge(out, "hue_light_color_hue", l, h);
        }
    }
    out.push('\n');

    write_header(
        out,
        "hue_light_color_saturation",
        "Color saturation percentage (0-100)",
        "gauge",
    );
    for l in lights {
        if let Some(s) = l.color_saturation {
            write_gauge(out, "hue_light_color_saturation", l, s);
        }
    }
    out.push('\n');

    // color temperature
    write_header(
        out,
        "hue_light_color_temperature_mirek",
        "Color temperature in mirek",
        "gauge",
    );
    for l in lights {
        if let Some(mirek) = l.color_temperature_mirek {
            write_gauge(out, "hue_light_color_temperature_mirek", l, mirek as f64);
        }
    }
    out.push('\n');

    write_header(
        out,
        "hue_light_color_temperature_mirek_valid",
        "Whether the mirek value is valid",
        "gauge",
    );
    for l in lights {
        if let Some(valid) = l.color_temperature_mirek_valid {
            write_gauge(
                out,
                "hue_light_color_temperature_mirek_valid",
                l,
                if valid { 1.0 } else { 0.0 },
            );
        }
    }
    out.push('\n');

    // dynamics
    write_header(
        out,
        "hue_light_dynamics_speed",
        "Light dynamics speed (0-1)",
        "gauge",
    );
    for l in lights {
        if let Some(speed) = l.dynamics_speed {
            write_gauge(out, "hue_light_dynamics_speed", l, speed);
        }
    }
    out.push('\n');

    // info metric — carries string-valued labels as a gauge with value 1
    write_header(out, "hue_light_info", "Light metadata", "gauge");
    for l in lights {
        if !l.reachable {
            continue;
        }
        let effect = l.effect.as_deref().unwrap_or("none");
        let dynamics = l.dynamics_status.as_deref().unwrap_or("none");
        let mode = l.mode.as_deref().unwrap_or("normal");

        use std::fmt::Write;
        let _ = writeln!(
            out,
            "hue_light_info{{{},effect=\"{}\",dynamics_status=\"{}\",mode=\"{}\"}} 1",
            labels(l),
            escape(effect),
            escape(dynamics),
            escape(mode),
        );
    }
}

fn write_header(out: &mut String, name: &str, help: &str, metric_type: &str) {
    use std::fmt::Write;
    let _ = write!(out, "# HELP {name} {help}\n# TYPE {name} {metric_type}\n");
}

fn write_gauge(out: &mut String, name: &str, light: &LightState, value: f64) {
    use std::fmt::Write;
    // Format without unnecessary trailing zeros, but always at least one decimal
    if value.fract() == 0.0 {
        let _ = writeln!(
            out,
            "{name}{{{labels}}} {val}",
            labels = labels(light),
            val = value as i64
        );
    } else {
        let _ = writeln!(out, "{name}{{{labels}}} {value}", labels = labels(light));
    }
}

fn labels(l: &LightState) -> String {
    format!(
        "room=\"{}\",light=\"{}\",device_id=\"{}\",mac=\"{}\",model_id=\"{}\",product=\"{}\"",
        escape(&l.room_name),
        escape(&l.light_name),
        escape(&l.device_id),
        escape(&l.mac_address),
        escape(&l.model_id),
        escape(&l.product_name),
    )
}

fn escape(s: &str) -> String {
    s.trim()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
