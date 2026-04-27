#![allow(unused_assignments)]
use datafusion_common::DataFusionError;
use error_stack_trace;
use iceberg_rust::error::Error as IcebergError;
use snafu::Location;
use snafu::prelude::*;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Snafu)]
#[snafu(visibility(pub(crate)))]
#[error_stack_trace::debug]
pub enum Error {
    #[snafu(display("Metastore error: {source}"))]
    Metastore {
        #[snafu(source(from(catalog_metastore::Error, Box::new)))]
        source: Box<catalog_metastore::Error>,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("DataFusion error: {error}"))]
    DataFusion {
        #[snafu(source(from(DataFusionError, Box::new)))]
        error: Box<DataFusionError>,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Invalid cache: Can't locate '{entity:?}' entity = {name}"))]
    InvalidCache {
        entity: String,
        name: String,
        #[snafu(implicit)]
        location: Location,
    },
    #[snafu(display("Feature not implemented: {feature:?}, {details:?}"))]
    NotImplemented {
        feature: UnsupportedFeature,
        details: String,
        #[snafu(implicit)]
        location: Location,
    },
    #[snafu(display("Missing volume with ident: '{name:?}'"))]
    MissingVolume {
        name: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Iceberg error: {error}"))]
    Iceberg {
        #[snafu(source(from(IcebergError, Box::new)))]
        error: Box<IcebergError>,
        #[snafu(implicit)]
        location: Location,
    },
}

#[allow(clippy::from_over_into)]
impl Into<IcebergError> for Error {
    fn into(self) -> IcebergError {
        IcebergError::External(Box::new(self))
    }
}

#[derive(Debug)]
pub enum UnsupportedFeature {}
