use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;

/// Assembled data for a single light, cross-referenced across API resources.
#[derive(Debug, Clone)]
pub struct LightState {
    pub light_name: String,
    pub room_name: String,
    pub device_id: String,
    pub mac_address: String,
    pub model_id: String,
    pub product_name: String,
    pub reachable: bool,
    pub on: Option<bool>,
    pub brightness: Option<f64>,
    pub color_xy: Option<(f64, f64)>,
    pub color_hue: Option<f64>,
    pub color_saturation: Option<f64>,
    pub color_temperature_mirek: Option<u32>,
    pub color_temperature_mirek_valid: Option<bool>,
    pub effect: Option<String>,
    pub dynamics_status: Option<String>,
    pub dynamics_speed: Option<f64>,
    pub mode: Option<String>,
}

/// Full snapshot of bridge state from a single poll cycle.
#[derive(Debug, Clone)]
pub struct BridgeState {
    pub lights: Vec<LightState>,
}

// -- Hue v2 API response types --

#[derive(Deserialize)]
struct ApiResponse<T> {
    data: Vec<T>,
}

#[derive(Deserialize)]
struct ResourceRef {
    rid: String,
}

// Light resource
#[derive(Deserialize)]
struct LightResource {
    id: String,
    on: Option<LightOn>,
    dimming: Option<LightDimming>,
    color: Option<LightColor>,
    color_temperature: Option<LightColorTemperature>,
    effects: Option<LightEffects>,
    dynamics: Option<LightDynamics>,
    mode: Option<String>,
    metadata: LightMetadata,
}

#[derive(Deserialize)]
struct LightOn {
    on: bool,
}

#[derive(Deserialize)]
struct LightDimming {
    brightness: f64,
}

#[derive(Deserialize)]
struct LightColor {
    xy: ColorXY,
}

#[derive(Deserialize)]
struct ColorXY {
    x: f64,
    y: f64,
}

#[derive(Deserialize)]
struct LightColorTemperature {
    mirek: Option<u32>,
    mirek_valid: Option<bool>,
}

#[derive(Deserialize)]
struct LightEffects {
    status: Option<String>,
}

#[derive(Deserialize)]
struct LightDynamics {
    status: Option<String>,
    speed: Option<f64>,
}

#[derive(Deserialize)]
struct LightMetadata {
    name: String,
}

// Device resource
#[derive(Deserialize)]
struct DeviceResource {
    id: String,
    product_data: Option<DeviceProductData>,
    services: Vec<ResourceRef>,
}

#[derive(Deserialize)]
struct DeviceProductData {
    model_id: Option<String>,
    product_name: Option<String>,
}

// Room resource
#[derive(Deserialize)]
struct RoomResource {
    children: Vec<ResourceRef>,
    metadata: RoomMetadata,
}

#[derive(Deserialize)]
struct RoomMetadata {
    name: String,
}

// Zigbee connectivity resource
#[derive(Deserialize)]
struct ZigbeeConnectivityResource {
    owner: ResourceRef,
    status: String,
    mac_address: Option<String>,
}

pub struct HueClient {
    client: Client,
    base_url: String,
    api_key: String,
}

impl HueClient {
    pub fn new(bridge_ip: &str, api_key: &str) -> Self {
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .expect("failed to build HTTP client");

        Self {
            client,
            base_url: format!("https://{bridge_ip}"),
            api_key: api_key.to_string(),
        }
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> reqwest::Result<Vec<T>> {
        let resp: ApiResponse<T> = self
            .client
            .get(format!("{}/clip/v2/resource/{}", self.base_url, path))
            .header("hue-application-key", &self.api_key)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp.data)
    }

    /// Poll all relevant resources and assemble a cross-referenced BridgeState.
    pub async fn poll(&self) -> Result<BridgeState, reqwest::Error> {
        let (lights, devices, rooms, zigbee) = tokio::try_join!(
            self.get::<LightResource>("light"),
            self.get::<DeviceResource>("device"),
            self.get::<RoomResource>("room"),
            self.get::<ZigbeeConnectivityResource>("zigbee_connectivity"),
        )?;

        // device_id -> (model_id, product_name)
        let mut device_info: HashMap<&str, (&str, &str)> = HashMap::new();
        for d in &devices {
            let model_id = d
                .product_data
                .as_ref()
                .and_then(|p| p.model_id.as_deref())
                .unwrap_or("unknown");
            let product_name = d
                .product_data
                .as_ref()
                .and_then(|p| p.product_name.as_deref())
                .unwrap_or("unknown");
            device_info.insert(&d.id, (model_id, product_name));
        }

        // light_id -> device_id
        let mut light_to_device: HashMap<&str, &str> = HashMap::new();
        for d in &devices {
            for svc in &d.services {
                light_to_device.insert(&svc.rid, &d.id);
            }
        }

        // device_id -> room_name
        let mut device_to_room: HashMap<&str, &str> = HashMap::new();
        for r in &rooms {
            for child in &r.children {
                device_to_room.insert(&child.rid, &r.metadata.name);
            }
        }

        // device_id -> (reachable, mac_address)
        let mut device_connectivity: HashMap<&str, (bool, &str)> = HashMap::new();
        for z in &zigbee {
            let reachable = z.status == "connected";
            let mac = z.mac_address.as_deref().unwrap_or("unknown");
            device_connectivity.insert(&z.owner.rid, (reachable, mac));
        }

        let mut result = Vec::with_capacity(lights.len());

        for light in &lights {
            let device_id = light_to_device.get(light.id.as_str()).copied().unwrap_or("unknown");
            let (model_id, product_name) = device_info
                .get(device_id)
                .copied()
                .unwrap_or(("unknown", "unknown"));
            let room_name = device_to_room.get(device_id).copied().unwrap_or("unknown");
            let (reachable, mac_address) = device_connectivity
                .get(device_id)
                .copied()
                .unwrap_or((false, "unknown"));

            let mut state = LightState {
                light_name: light.metadata.name.clone(),
                room_name: room_name.to_string(),
                device_id: device_id.to_string(),
                mac_address: mac_address.to_string(),
                model_id: model_id.to_string(),
                product_name: product_name.to_string(),
                reachable,
                on: None,
                brightness: None,
                color_xy: None,
                color_hue: None,
                color_saturation: None,
                color_temperature_mirek: None,
                color_temperature_mirek_valid: None,
                effect: None,
                dynamics_status: None,
                dynamics_speed: None,
                mode: None,
            };

            // Only populate detailed metrics if the light is reachable.
            if reachable {
                state.on = light.on.as_ref().map(|o| o.on);
                state.brightness = light.dimming.as_ref().map(|d| d.brightness);
                state.color_xy = light.color.as_ref().map(|c| (c.xy.x, c.xy.y));
                if let (Some((x, y)), Some(bri)) = (state.color_xy, state.brightness) {
                    let (h, s) = xy_brightness_to_hs(x, y, bri);
                    state.color_hue = Some(h);
                    state.color_saturation = Some(s);
                }
                state.color_temperature_mirek =
                    light.color_temperature.as_ref().and_then(|ct| ct.mirek);
                state.color_temperature_mirek_valid =
                    light.color_temperature.as_ref().and_then(|ct| ct.mirek_valid);
                state.effect = light.effects.as_ref().and_then(|e| e.status.clone());
                state.dynamics_status = light.dynamics.as_ref().and_then(|d| d.status.clone());
                state.dynamics_speed = light.dynamics.as_ref().and_then(|d| d.speed);
                state.mode = light.mode.clone();
            }

            result.push(state);
        }

        Ok(BridgeState { lights: result })
    }
}

/// Convert CIE xy + brightness to hue (degrees 0-360) and saturation (0-100%).
fn xy_brightness_to_hs(x: f64, y: f64, brightness: f64) -> (f64, f64) {
    // CIE xy + Y -> XYZ
    let cap_y = brightness / 100.0;
    let (cap_x, cap_z) = if y > 0.0 {
        ((cap_y / y) * x, (cap_y / y) * (1.0 - x - y))
    } else {
        (0.0, 0.0)
    };

    // XYZ -> linear sRGB (D65 reference white)
    let r_lin = cap_x * 3.2406 - cap_y * 1.5372 - cap_z * 0.4986;
    let g_lin = -cap_x * 0.9689 + cap_y * 1.8758 + cap_z * 0.0415;
    let b_lin = cap_x * 0.2104 - cap_y * 0.3500 + cap_z * 1.0572;

    // Clamp to [0, 1]
    let r = r_lin.clamp(0.0, 1.0);
    let g = g_lin.clamp(0.0, 1.0);
    let b = b_lin.clamp(0.0, 1.0);

    // RGB -> HSV
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let hue = if delta < f64::EPSILON {
        0.0
    } else if (max - r).abs() < f64::EPSILON {
        60.0 * (((g - b) / delta) % 6.0)
    } else if (max - g).abs() < f64::EPSILON {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    let hue = if hue < 0.0 { hue + 360.0 } else { hue };
    let saturation = if max < f64::EPSILON { 0.0 } else { (delta / max) * 100.0 };

    (hue, saturation)
}
