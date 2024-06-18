mod structs;
mod wifi;

use std::result::Result::Ok;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Result;

#[macro_use]
extern crate dotenv_codegen;
use embedded_svc::mqtt::client::QoS;
use esp_idf_hal::{delay::FreeRtos, gpio::PinDriver, peripherals::Peripherals};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::mqtt::client::EspMqttClient;
use esp_idf_svc::mqtt::client::EventPayload;
use esp_idf_svc::mqtt::client::MqttClientConfiguration;
use log::error;
use log::info;
use rgb::RGB8;
use structs::Config;
use structs::MqttMessage;
use wifi::try_reconnect_wifi;
use wifi::wifi;
use ws2812_esp32_rmt_driver::Ws2812Esp32Rmt;

const GREEN: RGB8 = rgb::RGB8::new(0, 128, 0);
const RED: RGB8 = rgb::RGB8::new(128, 0, 0);

fn main() -> Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;

    //Config IO
    let mut button = PinDriver::input(peripherals.pins.gpio19)?;
    button.set_pull(esp_idf_hal::gpio::Pull::Up)?;

    // Mutex to be able to share pointers
    let relay = Arc::new(Mutex::new(PinDriver::output(peripherals.pins.gpio10)?));
    relay
        .lock()
        .expect("Unable to lock pin mutex")
        .set_level(esp_idf_hal::gpio::Level::Low)?;
    let led_pin = peripherals.pins.gpio8;

    // Clone to create a reference for mqtt
    let relay_clone = Arc::clone(&relay);

    let channel = peripherals.rmt.channel0;
    let mut ws2812 = Ws2812Esp32Rmt::new(channel, led_pin)?;

    let pixels_red = std::iter::repeat(RED).take(25);
    ws2812.write_nocopy(pixels_red)?;

    let config = Config::new();

    //TODO: Reconnect if dont exist
    let mut wifi = wifi(&config.ssid, &config.password, peripherals.modem, sysloop)?;

    //MQTT
    // Set up handle for MQTT Config
    let mqtt_config = MqttClientConfiguration {
        client_id: Some(&config.client_id),
        crt_bundle_attach: Some(esp_idf_sys::esp_crt_bundle_attach),
        server_certificate: Some(config.server_cert),
        client_certificate: Some(config.client_cert),
        private_key: Some(config.private_key),
        ..Default::default()
    };

    // Create Client Instance and Define Behaviour on Event
    info!("Creating mqtt client");
    let mut client =
        EspMqttClient::new_cb(&config.mqtts_url, &mqtt_config, move |message_event| {
            match message_event.payload() {
                EventPayload::Connected(_) => info!("Connected"),
                EventPayload::Subscribed(id) => info!("Subscribed to id: {}", id),
                EventPayload::Received { data, .. } => {
                    if !data.is_empty() {
                        let mqtt_message: Result<MqttMessage, serde_json::Error> =
                            serde_json::from_slice(data);

                        match mqtt_message {
                            Ok(message) => {
                                info!("Recieved {:?}", message);

                                if message.message == "Hello from AWS IoT console" {
                                    info!("Activating relay from MQTT message");
                                    let mut relay =
                                        relay_clone.lock().expect("Unable to lock relay mutex");
                                    relay.set_high().expect("Unable to set relay to high");
                                    FreeRtos::delay_ms(5000);
                                    relay.set_low().expect("Unable to set relay to low");
                                }
                            }
                            Err(err) => error!(
                                "Could not parse message: {:?}. Err: {}",
                                std::str::from_utf8(data).unwrap(),
                                err
                            ),
                        }
                    }
                }
                _ => info!("{:?}", message_event.payload()),
            };
        })?;

    // Subscribe to MQTT Topic
    info!("Subscribing to topic");
    client.subscribe(&config.sub_topic, QoS::AtLeastOnce)?;

    info!("Starting main loop");

    let activated_message = MqttMessage {
        message: "Relay activated".into(),
    };

    let activated_json = serde_json::to_string(&activated_message)?;

    loop {
        // we are using thread::sleep here to make sure the watchdog isn't triggered
        FreeRtos::delay_ms(10);

        let pixel_color = std::iter::repeat(GREEN).take(25);

        if !wifi.is_connected()? {
            let pixel_color = std::iter::repeat(RED).take(25);
            ws2812.write_nocopy(pixel_color)?;

            try_reconnect_wifi(&mut wifi, &mut client, &config)?;
        }

        ws2812.write_nocopy(pixel_color)?;

        if button.is_low() {
            info!("Button pressed, activating relay");
            let mut relay = relay.lock().expect("Unable to lock relay mutex");
            relay.set_high()?;
            FreeRtos::delay_ms(5000);
            relay.set_low()?;
            client.publish(
                &config.pub_topic,
                QoS::AtLeastOnce,
                false,
                activated_json.as_bytes(),
            )?;
        }
    }
}
