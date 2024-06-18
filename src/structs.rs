use std::{mem, slice};

use esp_idf_svc::tls::X509;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct MqttMessage {
    pub message: String,
}

pub struct Config<'a> {
    pub ssid: String,
    pub password: String,
    pub client_id: String,
    pub server_cert: X509<'a>,
    pub client_cert: X509<'a>,
    pub private_key: X509<'a>,
    pub mqtts_url: String,
    pub sub_topic: String,
    pub pub_topic: String,
}

impl Config<'_> {
    pub fn new() -> Self {
        let server_cert_bytes: Vec<u8> = include_bytes!("../aws/AmazonRootCA1.pem").to_vec();
        let client_cert_bytes: Vec<u8> = include_bytes!("../aws/device.crt").to_vec();
        let private_key_bytes: Vec<u8> = include_bytes!("../aws/private.key").to_vec();

        let server_cert: X509 = convert_certificate(server_cert_bytes);
        let client_cert: X509 = convert_certificate(client_cert_bytes);
        let private_key: X509 = convert_certificate(private_key_bytes);

        Config {
            ssid: dotenv!("WIFI_SSID").into(),
            password: dotenv!("WIFI_PASSWORD").into(),
            client_id: dotenv!("CLIENT_ID").into(),
            server_cert,
            client_cert,
            private_key,
            mqtts_url: dotenv!("MQTTS_URL").into(),
            sub_topic: dotenv!("SUB_TOPIC").into(),
            pub_topic: dotenv!("PUB_TOPIC").into(),
        }
    }
}

fn convert_certificate(mut certificate_bytes: Vec<u8>) -> X509<'static> {
    // append NUL
    certificate_bytes.push(0);

    // convert the certificate
    let certificate_slice: &[u8] = unsafe {
        let ptr: *const u8 = certificate_bytes.as_ptr();
        let len: usize = certificate_bytes.len();
        mem::forget(certificate_bytes);

        slice::from_raw_parts(ptr, len)
    };
    // return the certificate file in the correct format
    X509::pem_until_nul(certificate_slice)
}
