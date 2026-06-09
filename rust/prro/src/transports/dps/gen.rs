//! Stable wrapper around the tonic-generated DPS module.
//!
//! The canonical proto uses `package com.programika.rro.ws.chk`, which is the
//! path `tonic::include_proto!` resolves. Keep generated types behind this
//! module so production code can later expose typed DTOs instead of leaking raw
//! tonic/prost shapes through the crate API.

#![allow(clippy::all)]
#![allow(missing_docs)]

tonic::include_proto!("com.programika.rro.ws.chk");
