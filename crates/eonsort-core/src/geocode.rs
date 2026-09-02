use crate::error::{Error, Result};
use ::exif::{In, Tag, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub const GAZETTEER_FILE: &str = "cities.txt";
pub const COUNTRIES_FILE: &str = "countryInfo.txt";
pub const REGIONS_FILE: &str = "admin1Codes.txt";
pub const CREDIT: &str = "Place names from GeoNames, CC BY 4.0, geonames.org";
pub const FOLDER: &str = "geonames";

pub use crate::weights::Progress as PlaceProgress;

pub struct Download {
    pub url: &'static str,
    pub member: Option<&'static str>,
    pub file: &'static str,
    pub bytes: u64,
}

pub const DOWNLOADS: [Download; 3] = [
    Download {
        url: "https://download.geonames.org/export/dump/cities500.zip",
        member: Some("cities500.txt"),
        file: GAZETTEER_FILE,
        bytes: 11_500_000,
    },
    Download {
        url: "https://download.geonames.org/export/dump/countryInfo.txt",
        member: None,
        file: COUNTRIES_FILE,
        bytes: 31_000,
    },
    Download {
        url: "https://download.geonames.org/export/dump/admin1CodesASCII.txt",
        member: None,
        file: REGIONS_FILE,
        bytes: 120_000,
    },
];

pub fn directory(models: &Path) -> PathBuf {
    models.join(FOLDER)
}

pub fn total_bytes() -> u64 {
    DOWNLOADS.iter().map(|d| d.bytes).sum()
}

pub fn present_bytes(models: &Path) -> u64 {
    let dir = directory(models);
    DOWNLOADS
        .iter()
        .filter_map(|d| std::fs::metadata(dir.join(d.file)).ok())
        .map(|meta| meta.len())
        .sum()
}

pub fn installed(models: &Path) -> bool {
    let dir = directory(models);
    DOWNLOADS.iter().all(|d| {
        std::fs::metadata(dir.join(d.file))
            .map(|meta| meta.is_file() && meta.len() > 0)
            .unwrap_or(false)
    })
}

pub fn remove(models: &Path) -> Result<()> {
    let dir = directory(models);
    for download in DOWNLOADS {
        let path = dir.join(download.file);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(Error::io(&path, e)),
        }
    }
    let _ = std::fs::remove_dir(&dir);
    Ok(())
}

#[cfg(feature = "download")]
pub fn download(
    models: &Path,
    cancel: &std::sync::atomic::AtomicBool,
    on_progress: &dyn Fn(PlaceProgress),
) -> Result<()> {
    let dir = directory(models);
    std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;

    for item in DOWNLOADS {
        let target = dir.join(item.file);
        if std::fs::metadata(&target)
            .map(|meta| meta.len() > 0)
            .unwrap_or(false)
        {
            continue;
        }

        match item.member {
            None => crate::weights::fetch_url(
                item.url,
                item.file,
                item.bytes,
                &target,
                cancel,
                on_progress,
            )?,
            Some(member) => {
                let archive = dir.join(format!("{}.zip.part", item.file));
                crate::weights::fetch_url(
                    item.url,
                    item.file,
                    item.bytes,
                    &archive,
                    cancel,
                    on_progress,
                )?;
                let unpacked = unzip(&archive, member, &target);
                let _ = std::fs::remove_file(&archive);
                unpacked?;
            }
        }
    }
    Ok(())
}

#[cfg(not(feature = "download"))]
pub fn download(
    _models: &Path,
    _cancel: &std::sync::atomic::AtomicBool,
    _on_progress: &dyn Fn(PlaceProgress),
) -> Result<()> {
    Err(Error::Download(
        "this build was made without downloading".into(),
    ))
}

#[cfg(feature = "download")]
fn unzip(archive: &Path, member: &str, target: &Path) -> Result<()> {
    let file = std::fs::File::open(archive).map_err(|e| Error::io(archive, e))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| Error::Download(format!("{} is not a readable archive: {e}", member)))?;
    let mut entry = zip
        .by_name(member)
        .map_err(|e| Error::Download(format!("the archive holds no {member}: {e}")))?;

    let part = target.with_extension("part");
    let mut out = std::fs::File::create(&part).map_err(|e| Error::io(&part, e))?;
    std::io::copy(&mut entry, &mut out).map_err(|e| Error::io(&part, e))?;
    out.sync_all().map_err(|e| Error::io(&part, e))?;
    drop(out);

    std::fs::rename(&part, target).map_err(|e| Error::io(target, e))
}
const EARTH_RADIUS_KM: f64 = 6_371.0;
const MAX_MATCH_KM: f64 = 120.0;
const CELL_DEGREES: f64 = 1.0;
const MAX_RINGS: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Coordinates {
    pub latitude: f64,
    pub longitude: f64,
}

impl Coordinates {
    pub fn new(latitude: f64, longitude: f64) -> Option<Self> {
        let sane = (-90.0..=90.0).contains(&latitude)
            && (-180.0..=180.0).contains(&longitude)
            && !(latitude == 0.0 && longitude == 0.0);
        sane.then_some(Self {
            latitude,
            longitude,
        })
    }

    pub fn distance_km(&self, other: &Coordinates) -> f64 {
        let (lat1, lat2) = (self.latitude.to_radians(), other.latitude.to_radians());
        let delta_lat = lat2 - lat1;
        let delta_lon = (other.longitude - self.longitude).to_radians();
        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
        2.0 * EARTH_RADIUS_KM * a.sqrt().clamp(-1.0, 1.0).asin()
    }

    fn cell(&self) -> (i32, i32) {
        (
            (self.latitude / CELL_DEGREES).floor() as i32,
            (self.longitude / CELL_DEGREES).floor() as i32,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Place {
    pub city: Option<String>,
    pub region: Option<String>,
    pub country: Option<String>,
    pub country_code: Option<String>,
}

impl Place {
    pub fn is_empty(&self) -> bool {
        self.city.is_none()
            && self.region.is_none()
            && self.country.is_none()
            && self.country_code.is_none()
    }
}

pub fn read(path: &Path) -> Option<Coordinates> {
    coordinates(&crate::exifread::from_path(path)?)
}

pub fn read_bytes(jpeg: &[u8]) -> Option<Coordinates> {
    coordinates(&crate::exifread::from_bytes(jpeg)?)
}

pub fn coordinates(exif: &::exif::Exif) -> Option<Coordinates> {
    let latitude = degrees(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef, 'S')?;
    let longitude = degrees(exif, Tag::GPSLongitude, Tag::GPSLongitudeRef, 'W')?;
    Coordinates::new(latitude, longitude)
}

fn degrees(exif: &::exif::Exif, value: Tag, reference: Tag, negative: char) -> Option<f64> {
    let field = exif.get_field(value, In::PRIMARY)?;
    let Value::Rational(ref parts) = field.value else {
        return None;
    };
    if parts.len() < 3 {
        return None;
    }

    let part = |index: usize| -> Option<f64> {
        let value = parts.get(index)?;
        (value.denom != 0).then(|| value.num as f64 / value.denom as f64)
    };
    let magnitude = part(0)? + part(1)? / 60.0 + part(2)? / 3_600.0;

    let sign = exif
        .get_field(reference, In::PRIMARY)
        .map(|f| f.display_value().to_string())
        .and_then(|raw| raw.trim().trim_matches('"').chars().next())
        .map(|c| {
            if c.to_ascii_uppercase() == negative {
                -1.0
            } else {
                1.0
            }
        })
        .unwrap_or(1.0);

    Some(sign * magnitude)
}

#[derive(Debug, Clone, PartialEq)]
struct Entry {
    name: String,
    at: Coordinates,
    country_code: String,
    region: Option<String>,
    population: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Gazetteer {
    cells: HashMap<(i32, i32), Vec<Entry>>,
    countries: HashMap<String, String>,
    regions: HashMap<String, String>,
    entries: usize,
}

impl Gazetteer {
    pub fn len(&self) -> usize {
        self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries == 0
    }

    pub fn load(directory: &Path) -> Result<Self> {
        let cities = directory.join(GAZETTEER_FILE);
        if !cities.is_file() {
            return Err(Error::io(
                &cities,
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "no gazetteer to look place names up in",
                ),
            ));
        }

        let mut gazetteer = Self::from_cities(&cities)?;
        let countries = directory.join(COUNTRIES_FILE);
        if countries.is_file() {
            gazetteer.countries = read_countries(&countries)?;
        }
        let regions = directory.join(REGIONS_FILE);
        if regions.is_file() {
            gazetteer.regions = read_regions(&regions)?;
        }
        Ok(gazetteer)
    }

    pub fn region_name(&self, country_code: &str, code: &str) -> Option<String> {
        self.regions.get(&format!("{country_code}.{code}")).cloned()
    }

    pub fn from_cities(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|e| Error::io(path, e))?;
        let mut gazetteer = Self::default();

        for line in BufReader::new(file).lines() {
            let line = line.map_err(|e| Error::io(path, e))?;
            let Some(entry) = parse_city(&line) else {
                continue;
            };
            gazetteer
                .cells
                .entry(entry.at.cell())
                .or_default()
                .push(entry);
            gazetteer.entries += 1;
        }
        Ok(gazetteer)
    }

    pub fn country_name(&self, code: &str) -> Option<String> {
        self.countries.get(code).cloned()
    }

    pub fn place(&self, at: Coordinates) -> Place {
        let Some((entry, _)) = self.nearest(at) else {
            return Place::default();
        };
        Place {
            city: Some(entry.name.clone()),
            region: entry
                .region
                .as_deref()
                .and_then(|code| self.region_name(&entry.country_code, code))
                .or_else(|| entry.region.clone()),
            country: self
                .country_name(&entry.country_code)
                .or_else(|| Some(entry.country_code.clone())),
            country_code: Some(entry.country_code.clone()),
        }
    }

    fn nearest(&self, at: Coordinates) -> Option<(&Entry, f64)> {
        let (lat_cell, lon_cell) = at.cell();

        for ring in 0..=MAX_RINGS {
            let mut best: Option<(&Entry, f64)> = None;
            for lat in lat_cell - ring..=lat_cell + ring {
                for lon in lon_cell - ring..=lon_cell + ring {
                    if ring > 0 && (lat - lat_cell).abs() < ring && (lon - lon_cell).abs() < ring {
                        continue;
                    }
                    let Some(entries) = self.cells.get(&(lat, wrap_longitude(lon))) else {
                        continue;
                    };
                    for entry in entries {
                        let distance = at.distance_km(&entry.at);
                        if distance > MAX_MATCH_KM {
                            continue;
                        }
                        let better = match best {
                            None => true,
                            Some((current, current_distance)) => {
                                distance < current_distance
                                    || (distance == current_distance
                                        && entry.population > current.population)
                            }
                        };
                        if better {
                            best = Some((entry, distance));
                        }
                    }
                }
            }
            if best.is_some() {
                return best;
            }
        }
        None
    }
}

fn wrap_longitude(cell: i32) -> i32 {
    let span = (360.0 / CELL_DEGREES) as i32;
    let half = span / 2;
    ((cell + half).rem_euclid(span)) - half
}

fn parse_city(line: &str) -> Option<Entry> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() < 15 {
        return None;
    }
    let name = fields[1].trim();
    if name.is_empty() {
        return None;
    }
    let at = Coordinates::new(fields[4].parse().ok()?, fields[5].parse().ok()?)?;
    let country_code = fields[8].trim();
    if country_code.is_empty() {
        return None;
    }
    let region = match fields[10].trim() {
        "" => None,
        value => Some(value.to_string()),
    };

    Some(Entry {
        name: name.to_string(),
        at,
        country_code: country_code.to_string(),
        region,
        population: fields[14].parse().unwrap_or(0),
    })
}

fn read_regions(path: &Path) -> Result<HashMap<String, String>> {
    let file = File::open(path).map_err(|e| Error::io(path, e))?;
    let mut names = HashMap::new();

    for line in BufReader::new(file).lines() {
        let line = line.map_err(|e| Error::io(path, e))?;
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 2 {
            continue;
        }
        let (key, name) = (fields[0].trim(), fields[1].trim());
        if key.contains('.') && !name.is_empty() {
            names.insert(key.to_string(), name.to_string());
        }
    }
    Ok(names)
}

fn read_countries(path: &Path) -> Result<HashMap<String, String>> {
    let file = File::open(path).map_err(|e| Error::io(path, e))?;
    let mut names = HashMap::new();

    for line in BufReader::new(file).lines() {
        let line = line.map_err(|e| Error::io(path, e))?;
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 5 {
            continue;
        }
        let (code, name) = (fields[0].trim(), fields[4].trim());
        if code.len() == 2 && !name.is_empty() {
            names.insert(code.to_string(), name.to_string());
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn city(name: &str, lat: f64, lon: f64, code: &str, admin: &str, population: u64) -> String {
        let mut fields = vec![String::new(); 19];
        fields[1] = name.to_string();
        fields[4] = lat.to_string();
        fields[5] = lon.to_string();
        fields[8] = code.to_string();
        fields[10] = admin.to_string();
        fields[14] = population.to_string();
        fields.join("\t")
    }

    fn gazetteer(lines: &[String]) -> (tempfile::TempDir, Gazetteer) {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(GAZETTEER_FILE), lines.join("\n")).unwrap();
        let loaded = Gazetteer::load(dir.path()).unwrap();
        (dir, loaded)
    }

    #[test]
    fn a_pair_of_coordinates_has_to_be_on_the_globe() {
        assert!(Coordinates::new(48.1, 11.6).is_some());
        assert!(Coordinates::new(-90.0, 180.0).is_some());
        assert!(Coordinates::new(91.0, 0.0).is_none());
        assert!(Coordinates::new(0.0, 181.0).is_none());
    }

    #[test]
    fn the_null_island_reading_a_stripped_camera_leaves_behind_is_refused() {
        assert!(Coordinates::new(0.0, 0.0).is_none());
    }

    #[test]
    fn distance_between_two_points_is_the_great_circle_one() {
        let munich = Coordinates::new(48.137, 11.576).unwrap();
        let berlin = Coordinates::new(52.520, 13.405).unwrap();
        let apart = munich.distance_km(&berlin);
        assert!((apart - 504.0).abs() < 10.0, "{apart}");
        assert_eq!(munich.distance_km(&munich), 0.0);
    }

    #[test]
    fn finds_the_nearest_place_to_a_reading() {
        let (_dir, gazetteer) = gazetteer(&[
            city("Munich", 48.137, 11.576, "DE", "02", 1_500_000),
            city("Berlin", 52.520, 13.405, "DE", "16", 3_600_000),
        ]);

        let place = gazetteer.place(Coordinates::new(48.2, 11.6).unwrap());
        assert_eq!(place.city.as_deref(), Some("Munich"));
        assert_eq!(place.country_code.as_deref(), Some("DE"));
        assert_eq!(place.region.as_deref(), Some("02"));
    }

    #[test]
    fn a_reading_in_the_middle_of_an_ocean_names_nowhere() {
        let (_dir, gazetteer) = gazetteer(&[city("Munich", 48.137, 11.576, "DE", "02", 1)]);

        let place = gazetteer.place(Coordinates::new(-40.0, -140.0).unwrap());
        assert!(place.is_empty());
    }

    #[test]
    fn a_place_further_than_the_match_radius_is_not_claimed() {
        let (_dir, gazetteer) = gazetteer(&[city("Munich", 48.137, 11.576, "DE", "02", 1)]);

        let near = gazetteer.place(Coordinates::new(48.6, 11.9).unwrap());
        assert_eq!(near.city.as_deref(), Some("Munich"));

        let far = gazetteer.place(Coordinates::new(52.520, 13.405).unwrap());
        assert!(far.is_empty());
    }

    #[test]
    fn the_closer_of_two_neighbours_wins() {
        let (_dir, gazetteer) = gazetteer(&[
            city("Near", 48.10, 11.50, "DE", "02", 10),
            city("Far", 48.50, 11.90, "DE", "02", 9_000_000),
        ]);

        let place = gazetteer.place(Coordinates::new(48.11, 11.51).unwrap());
        assert_eq!(place.city.as_deref(), Some("Near"));
    }

    #[test]
    fn a_search_widens_past_its_own_cell() {
        let (_dir, gazetteer) = gazetteer(&[city("Munich", 48.137, 11.576, "DE", "02", 1)]);

        let place = gazetteer.place(Coordinates::new(49.05, 11.576).unwrap());
        assert_eq!(place.city.as_deref(), Some("Munich"));
    }

    #[test]
    fn country_names_come_from_the_companion_file_when_it_is_there() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join(GAZETTEER_FILE),
            city("Munich", 48.137, 11.576, "DE", "02", 1),
        )
        .unwrap();
        std::fs::write(
            dir.path().join(COUNTRIES_FILE),
            "#ISO\tISO3\tnum\tfips\tCountry\tCapital\nDE\tDEU\t276\tGM\tGermany\tBerlin\n",
        )
        .unwrap();

        let gazetteer = Gazetteer::load(dir.path()).unwrap();
        let place = gazetteer.place(Coordinates::new(48.137, 11.576).unwrap());
        assert_eq!(place.country.as_deref(), Some("Germany"));
    }

    #[test]
    fn without_the_companion_file_the_country_falls_back_to_its_code() {
        let (_dir, gazetteer) = gazetteer(&[city("Munich", 48.137, 11.576, "DE", "02", 1)]);
        let place = gazetteer.place(Coordinates::new(48.137, 11.576).unwrap());
        assert_eq!(place.country.as_deref(), Some("DE"));
    }

    #[test]
    fn loading_without_a_gazetteer_says_so_rather_than_pretending() {
        let dir = tempdir().unwrap();
        assert!(Gazetteer::load(dir.path()).is_err());
        assert!(Gazetteer::default().is_empty());
        assert!(Gazetteer::default()
            .place(Coordinates::new(48.1, 11.6).unwrap())
            .is_empty());
    }

    #[test]
    fn rubbish_lines_are_stepped_over_rather_than_failing_the_load() {
        let (_dir, gazetteer) = gazetteer(&[
            "not a gazetteer line".to_string(),
            String::new(),
            city("Munich", 48.137, 11.576, "DE", "02", 1),
            city("", 1.0, 1.0, "XX", "", 0),
        ]);
        assert_eq!(gazetteer.len(), 1);
    }

    #[test]
    fn longitude_cells_wrap_around_the_date_line() {
        assert_eq!(wrap_longitude(0), 0);
        assert_eq!(wrap_longitude(179), 179);
        assert_eq!(wrap_longitude(-180), -180);
        assert_eq!(wrap_longitude(180), -180);
        assert_eq!(wrap_longitude(-181), 179);
    }

    #[test]
    fn reads_coordinates_out_of_a_real_exif_block() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("located.jpg");
        std::fs::write(&path, crate::exif_write::jpeg_with_gps(48.137, 11.576)).unwrap();

        let found = read(&path).unwrap();
        assert!((found.latitude - 48.137).abs() < 0.001, "{found:?}");
        assert!((found.longitude - 11.576).abs() < 0.001, "{found:?}");
    }

    #[test]
    fn reads_a_southern_and_western_reading_as_negative() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("rio.jpg");
        std::fs::write(&path, crate::exif_write::jpeg_with_gps(-22.906, -43.172)).unwrap();

        let found = read(&path).unwrap();
        assert!(found.latitude < 0.0 && found.longitude < 0.0, "{found:?}");
        assert!((found.latitude + 22.906).abs() < 0.001, "{found:?}");
        assert!((found.longitude + 43.172).abs() < 0.001, "{found:?}");
    }

    #[test]
    fn a_picture_with_no_gps_block_reads_as_nowhere() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("plain.jpg");
        std::fs::write(&path, crate::exif_write::jpeg_with_exif(16, 16, 1)).unwrap();
        assert!(read(&path).is_none());
    }

    #[test]
    fn the_gazetteer_lives_in_its_own_folder_under_the_models() {
        let dir = directory(Path::new("/data/models"));
        assert!(dir.ends_with("geonames"));
        assert_eq!(dir.parent().unwrap(), Path::new("/data/models"));
    }

    #[test]
    fn nothing_is_installed_in_an_empty_folder() {
        let dir = tempdir().unwrap();
        assert!(!installed(dir.path()));
        assert_eq!(present_bytes(dir.path()), 0);
        assert!(total_bytes() > 0);
    }

    #[test]
    fn every_file_has_to_be_there_before_it_counts_as_installed() {
        let dir = tempdir().unwrap();
        let places = directory(dir.path());
        std::fs::create_dir_all(&places).unwrap();

        for (index, item) in DOWNLOADS.iter().enumerate() {
            assert!(!installed(dir.path()), "after {index} of the files");
            std::fs::write(places.join(item.file), "x").unwrap();
        }

        assert!(installed(dir.path()));
        assert_eq!(present_bytes(dir.path()), DOWNLOADS.len() as u64);
    }

    #[test]
    fn a_half_written_file_does_not_count_as_installed() {
        let dir = tempdir().unwrap();
        let places = directory(dir.path());
        std::fs::create_dir_all(&places).unwrap();
        std::fs::write(places.join(GAZETTEER_FILE), "").unwrap();
        std::fs::write(places.join(COUNTRIES_FILE), "bb").unwrap();

        assert!(!installed(dir.path()));
    }

    #[test]
    fn removing_clears_both_files_and_the_folder() {
        let dir = tempdir().unwrap();
        let places = directory(dir.path());
        std::fs::create_dir_all(&places).unwrap();
        for item in DOWNLOADS {
            std::fs::write(places.join(item.file), "x").unwrap();
        }

        remove(dir.path()).unwrap();

        assert!(!installed(dir.path()));
        assert!(!places.exists());
    }

    #[test]
    fn removing_what_is_not_there_is_not_an_error() {
        let dir = tempdir().unwrap();
        assert!(remove(dir.path()).is_ok());
    }

    #[test]
    fn every_download_names_a_geonames_file_and_a_size() {
        for item in DOWNLOADS {
            assert!(
                item.url.starts_with("https://download.geonames.org/"),
                "{}",
                item.url
            );
            assert!(item.bytes > 0, "{}", item.file);
            assert!(!item.file.is_empty());
        }
        assert!(DOWNLOADS.iter().any(|d| d.member.is_some()));
    }

    #[test]
    fn the_source_of_the_place_names_is_credited() {
        assert!(CREDIT.contains("GeoNames"));
        assert!(CREDIT.contains("CC BY"));
    }

    #[cfg(feature = "download")]
    #[test]
    fn a_downloaded_archive_gives_up_the_file_inside_it() {
        let dir = tempdir().unwrap();
        let archive = dir.path().join("cities.zip");
        let body = b"1	Munich			48.137	11.576	P	PPL	DE		02				1500000				";
        std::fs::write(&archive, stored_zip("cities500.txt", body)).unwrap();

        let target = dir.path().join(GAZETTEER_FILE);
        unzip(&archive, "cities500.txt", &target).unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), body);
        let loaded = Gazetteer::from_cities(&target).unwrap();
        assert_eq!(loaded.len(), 1);
    }

    #[cfg(feature = "download")]
    #[test]
    fn an_archive_without_the_expected_file_is_refused() {
        let dir = tempdir().unwrap();
        let archive = dir.path().join("cities.zip");
        std::fs::write(&archive, stored_zip("something_else.txt", b"nope")).unwrap();

        let target = dir.path().join(GAZETTEER_FILE);
        assert!(unzip(&archive, "cities500.txt", &target).is_err());
        assert!(!target.exists());
    }

    #[cfg(feature = "download")]
    #[test]
    fn bytes_that_are_not_an_archive_are_refused() {
        let dir = tempdir().unwrap();
        let archive = dir.path().join("cities.zip");
        std::fs::write(&archive, b"not a zip at all").unwrap();

        assert!(unzip(&archive, "cities500.txt", &dir.path().join("out.txt")).is_err());
    }

    #[cfg(feature = "download")]
    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for byte in data {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    #[cfg(feature = "download")]
    fn stored_zip(name: &str, body: &[u8]) -> Vec<u8> {
        let crc = crc32(body);
        let size = body.len() as u32;
        let name_bytes = name.as_bytes();
        let mut out = Vec::new();

        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(body);

        let central = out.len() as u32;
        out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(name_bytes);

        let central_size = out.len() as u32 - central;
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&central_size.to_le_bytes());
        out.extend_from_slice(&central.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    #[test]
    fn a_region_code_is_swapped_for_its_name_when_the_mapping_is_there() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join(GAZETTEER_FILE),
            city("Munich", 48.137, 11.576, "DE", "02", 1),
        )
        .unwrap();
        std::fs::write(
            dir.path().join(REGIONS_FILE),
            "DE.02	Bavaria	Bavaria	2951839
",
        )
        .unwrap();

        let gazetteer = Gazetteer::load(dir.path()).unwrap();
        let place = gazetteer.place(Coordinates::new(48.137, 11.576).unwrap());
        assert_eq!(place.region.as_deref(), Some("Bavaria"));
    }

    #[test]
    fn without_the_mapping_the_region_stays_the_bare_code() {
        let (_dir, gazetteer) = gazetteer(&[city("Munich", 48.137, 11.576, "DE", "02", 1)]);
        let place = gazetteer.place(Coordinates::new(48.137, 11.576).unwrap());
        assert_eq!(place.region.as_deref(), Some("02"));
    }

    #[test]
    fn a_region_of_one_country_is_never_read_as_another_countrys() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join(GAZETTEER_FILE),
            city("Somewhere", 48.137, 11.576, "FR", "02", 1),
        )
        .unwrap();
        std::fs::write(
            dir.path().join(REGIONS_FILE),
            "DE.02	Bavaria		1
",
        )
        .unwrap();

        let gazetteer = Gazetteer::load(dir.path()).unwrap();
        let place = gazetteer.place(Coordinates::new(48.137, 11.576).unwrap());
        assert_eq!(place.region.as_deref(), Some("02"));
        assert_eq!(
            gazetteer.region_name("DE", "02").as_deref(),
            Some("Bavaria")
        );
    }
}
