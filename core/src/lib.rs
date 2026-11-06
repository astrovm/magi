//! Everything Magi does. Cloudflare stays behind the [`platform::Platform`]
//! trait, so all of this runs in plain native tests.

pub mod alias;
pub mod guards;
pub mod html;
pub mod pages;
pub mod platform;
pub mod qr;
pub mod responses;
pub mod routes;
pub mod secrets;
pub mod spells;
pub mod store;
pub mod target;
pub mod telegram;
pub mod text;
pub mod visits;
pub mod words;
