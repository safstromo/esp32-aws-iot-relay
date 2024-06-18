use anyhow::{bail, Result};
use esp_idf_hal::{delay::FreeRtos, peripheral};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    mqtt::client::{EspMqttClient, QoS},
    nvs::EspDefaultNvsPartition,
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use esp_idf_sys::EspError;
use log::info;

use crate::structs::Config;

pub fn wifi(
    ssid: &str,
    pass: &str,
    modem: impl peripheral::Peripheral<P = esp_idf_svc::hal::modem::Modem> + 'static,
    sysloop: EspSystemEventLoop,
) -> Result<Box<EspWifi<'static>>> {
    let nvs = EspDefaultNvsPartition::take()?;

    let mut auth_method = AuthMethod::WPA2Personal;
    if ssid.is_empty() {
        bail!("Missing WiFi name")
    }
    if pass.is_empty() {
        auth_method = AuthMethod::None;
        info!("Wifi password is empty");
    }
    let mut esp_wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs))?;

    let mut wifi = BlockingWifi::wrap(&mut esp_wifi, sysloop)?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration::default()))?;

    info!("Starting wifi...");

    wifi.start()?;

    info!("Scanning...");

    let ap_infos = wifi.scan()?;

    let access_point = ap_infos.into_iter().find(|a| a.ssid == ssid);

    let channel = if let Some(access_point) = access_point {
        info!(
            "Found configured access point with SSID:{} on channel {}",
            ssid, access_point.channel
        );
        Some(access_point.channel)
    } else {
        info!(
            "Configured access point with SSID:{} not found during scanning, will go with unknown channel",
            ssid
        );
        None
    };

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: ssid.try_into().expect("Was not able to convert ssid"),
        password: pass.try_into().expect("Was not able to convert password"),
        channel,
        auth_method,
        ..Default::default()
    }))?;

    info!("Connecting wifi...");

    wifi.connect()?;

    info!("Waiting for DHCP lease...");

    wifi.wait_netif_up()?;

    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;

    info!("Wifi DHCP info: {:?}", ip_info);

    Ok(Box::new(esp_wifi))
}

pub fn try_reconnect_wifi(
    wifi: &mut Box<EspWifi<'static>>,
    mqtt_client: &mut EspMqttClient<'static>,
    config: &Config,
) -> Result<(), EspError> {
    info!("Wifi disconnected");

    while !wifi.is_connected().unwrap() {
        info!("Reconnecting...");
        if wifi.as_mut().connect().is_err() {
            info!("No access point found, Sleeping for 10sec",);
            FreeRtos::delay_ms(10000);
        }
    }

    // Sleep to let mqtt client reconnect
    FreeRtos::delay_ms(10000);
    info!("Resubscribing to topic...");
    mqtt_client.subscribe(&config.sub_topic, QoS::AtLeastOnce)?;
    Ok(())
}
