use crate::error::{Error, Result};
use crate::geocode::{Coordinates, Place};
use chrono::NaiveDateTime;
use std::path::{Path, PathBuf};

pub const SIDECAR_EXTENSION: &str = "xmp";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sidecar {
    pub taken: Option<NaiveDateTime>,
    pub at: Option<Coordinates>,
    pub place: Place,
    pub tags: Vec<String>,
    pub people: Vec<String>,
    pub caption: Option<String>,
}

impl Sidecar {
    pub fn is_empty(&self) -> bool {
        self.taken.is_none()
            && self.at.is_none()
            && self.place.is_empty()
            && self.tags.is_empty()
            && self.people.is_empty()
            && self.caption.is_none()
    }
}

pub fn sidecar_path(image: &Path) -> PathBuf {
    let mut name = image.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(SIDECAR_EXTENSION);
    image.with_file_name(name)
}

pub fn write(image: &Path, sidecar: &Sidecar) -> Result<Option<PathBuf>> {
    if sidecar.is_empty() {
        return Ok(None);
    }
    let path = sidecar_path(image);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
    }
    std::fs::write(&path, render(sidecar)).map_err(|e| Error::io(&path, e))?;
    Ok(Some(path))
}

pub fn render(sidecar: &Sidecar) -> String {
    let mut attributes = Vec::new();

    if let Some(taken) = sidecar.taken {
        let stamp = taken.format("%Y-%m-%dT%H:%M:%S").to_string();
        attributes.push(format!("xmp:CreateDate=\"{stamp}\""));
        attributes.push(format!("xmp:ModifyDate=\"{stamp}\""));
        attributes.push(format!("exif:DateTimeOriginal=\"{stamp}\""));
        attributes.push(format!("photoshop:DateCreated=\"{stamp}\""));
    }

    if let Some(at) = sidecar.at {
        attributes.push(format!(
            "exif:GPSLatitude=\"{}\"",
            decimal_minutes(at.latitude, 'N', 'S')
        ));
        attributes.push(format!(
            "exif:GPSLongitude=\"{}\"",
            decimal_minutes(at.longitude, 'E', 'W')
        ));
        attributes.push("exif:GPSVersionID=\"2.2.0.0\"".to_string());
    }

    if let Some(city) = sidecar.place.city.as_deref() {
        attributes.push(format!("photoshop:City=\"{}\"", escape(city)));
    }
    if let Some(region) = sidecar.place.region.as_deref() {
        attributes.push(format!("photoshop:State=\"{}\"", escape(region)));
    }
    if let Some(country) = sidecar.place.country.as_deref() {
        attributes.push(format!("photoshop:Country=\"{}\"", escape(country)));
    }
    if let Some(code) = sidecar.place.country_code.as_deref() {
        attributes.push(format!("Iptc4xmpCore:CountryCode=\"{}\"", escape(code)));
    }

    let mut elements = String::new();
    if !sidecar.tags.is_empty() {
        elements.push_str(&bag("dc:subject", &sidecar.tags));
    }
    if !sidecar.people.is_empty() {
        elements.push_str(&bag("Iptc4xmpExt:PersonInImage", &sidecar.people));
    }
    if let Some(caption) = sidecar.caption.as_deref() {
        elements.push_str(&alt("dc:description", caption));
    }

    let attribute_block = attributes
        .iter()
        .map(|a| format!("\n    {a}"))
        .collect::<String>();

    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"eonsort\">\n\
         \x20<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
         \x20 <rdf:Description rdf:about=\"\"\n\
         \x20   xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"\n\
         \x20   xmlns:exif=\"http://ns.adobe.com/exif/1.0/\"\n\
         \x20   xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\n\
         \x20   xmlns:photoshop=\"http://ns.adobe.com/photoshop/1.0/\"\n\
         \x20   xmlns:Iptc4xmpCore=\"http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/\"\n\
         \x20   xmlns:Iptc4xmpExt=\"http://iptc.org/std/Iptc4xmpExt/2008-02-29/\"{attribute_block}>\n\
         {elements}\
         \x20 </rdf:Description>\n\
         \x20</rdf:RDF>\n\
         </x:xmpmeta>\n\
         <?xpacket end=\"w\"?>\n"
    )
}

fn bag(name: &str, values: &[String]) -> String {
    let items = values
        .iter()
        .map(|value| format!("     <rdf:li>{}</rdf:li>\n", escape(value)))
        .collect::<String>();
    format!("   <{name}>\n    <rdf:Bag>\n{items}    </rdf:Bag>\n   </{name}>\n")
}

fn alt(name: &str, value: &str) -> String {
    format!(
        "   <{name}>\n    <rdf:Alt>\n     <rdf:li xml:lang=\"x-default\">{}</rdf:li>\n    </rdf:Alt>\n   </{name}>\n",
        escape(value)
    )
}

fn decimal_minutes(value: f64, positive: char, negative: char) -> String {
    let magnitude = value.abs();
    let degrees = magnitude.floor();
    let minutes = (magnitude - degrees) * 60.0;
    let letter = if value < 0.0 { negative } else { positive };
    format!("{},{:.6}{letter}", degrees as u32, minutes)
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use tempfile::tempdir;

    fn taken() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2019, 7, 4)
            .unwrap()
            .and_hms_opt(10, 30, 5)
            .unwrap()
    }

    fn full() -> Sidecar {
        Sidecar {
            taken: Some(taken()),
            at: Coordinates::new(48.137, 11.576),
            place: Place {
                city: Some("Munich".to_string()),
                region: Some("Bavaria".to_string()),
                country: Some("Germany".to_string()),
                country_code: Some("DE".to_string()),
            },
            tags: vec!["beach".to_string(), "sunset".to_string()],
            people: vec!["Grandma".to_string()],
            caption: Some("A day at the lake".to_string()),
        }
    }

    #[test]
    fn the_sidecar_sits_beside_the_picture_keeping_its_full_name() {
        assert_eq!(
            sidecar_path(Path::new("/out/2019/07/IMG_1.jpg")),
            PathBuf::from("/out/2019/07/IMG_1.jpg.xmp")
        );
        assert_eq!(
            sidecar_path(Path::new("/out/IMG_1.CR2")),
            PathBuf::from("/out/IMG_1.CR2.xmp")
        );
    }

    #[test]
    fn the_date_is_written_in_the_form_every_reader_expects() {
        let rendered = render(&full());
        assert!(rendered.contains("xmp:CreateDate=\"2019-07-04T10:30:05\""));
        assert!(rendered.contains("exif:DateTimeOriginal=\"2019-07-04T10:30:05\""));
        assert!(rendered.contains("photoshop:DateCreated=\"2019-07-04T10:30:05\""));
    }

    #[test]
    fn a_reading_is_written_as_degrees_and_decimal_minutes() {
        let rendered = render(&full());
        assert!(
            rendered.contains("exif:GPSLatitude=\"48,8.220000N\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("exif:GPSLongitude=\"11,34.560000E\""),
            "{rendered}"
        );
    }

    #[test]
    fn a_southern_western_reading_carries_the_other_two_letters() {
        let sidecar = Sidecar {
            at: Coordinates::new(-22.906, -43.172),
            ..Sidecar::default()
        };
        let rendered = render(&sidecar);
        assert!(
            rendered.contains("GPSLatitude=\"22,54.360000S\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("GPSLongitude=\"43,10.320000W\""),
            "{rendered}"
        );
    }

    #[test]
    fn the_place_names_land_in_the_fields_cataloguers_read() {
        let rendered = render(&full());
        assert!(rendered.contains("photoshop:City=\"Munich\""));
        assert!(rendered.contains("photoshop:State=\"Bavaria\""));
        assert!(rendered.contains("photoshop:Country=\"Germany\""));
        assert!(rendered.contains("Iptc4xmpCore:CountryCode=\"DE\""));
    }

    #[test]
    fn tags_and_people_are_written_as_the_bags_they_are() {
        let rendered = render(&full());
        assert!(rendered.contains("<dc:subject>"));
        assert!(rendered.contains("<rdf:li>beach</rdf:li>"));
        assert!(rendered.contains("<rdf:li>sunset</rdf:li>"));
        assert!(rendered.contains("<Iptc4xmpExt:PersonInImage>"));
        assert!(rendered.contains("<rdf:li>Grandma</rdf:li>"));
    }

    #[test]
    fn a_caption_is_written_as_a_language_alternative() {
        let rendered = render(&full());
        assert!(rendered.contains("<dc:description>"));
        assert!(rendered.contains("xml:lang=\"x-default\">A day at the lake<"));
    }

    #[test]
    fn a_name_with_xml_in_it_cannot_break_the_file() {
        let sidecar = Sidecar {
            people: vec!["<script>&\"bad\"</script>".to_string()],
            ..Sidecar::default()
        };
        let rendered = render(&sidecar);
        assert!(!rendered.contains("<script>"), "{rendered}");
        assert!(rendered.contains("&lt;script&gt;&amp;&quot;bad&quot;"));
    }

    #[test]
    fn a_control_character_is_dropped_rather_than_written() {
        let sidecar = Sidecar {
            caption: Some("bell\u{7}here".to_string()),
            ..Sidecar::default()
        };
        assert!(render(&sidecar).contains("bellhere"));
    }

    #[test]
    fn the_packet_is_opened_and_closed_the_way_readers_look_for() {
        let rendered = render(&full());
        assert!(rendered.starts_with("<?xpacket begin="));
        assert!(rendered.trim_end().ends_with("<?xpacket end=\"w\"?>"));
        assert!(rendered.contains("</x:xmpmeta>"));
        assert_eq!(rendered.matches("<rdf:Description").count(), 1);
    }

    #[test]
    fn every_element_that_opens_also_closes() {
        let rendered = render(&full());
        for name in [
            "x:xmpmeta",
            "rdf:RDF",
            "rdf:Description",
            "dc:subject",
            "rdf:Bag",
            "Iptc4xmpExt:PersonInImage",
            "dc:description",
            "rdf:Alt",
        ] {
            assert_eq!(
                rendered.matches(&format!("</{name}>")).count(),
                rendered.matches(&format!("<{name}>")).count().max(1),
                "{name} in {rendered}"
            );
        }
    }

    #[test]
    fn writing_puts_the_file_next_to_the_picture() {
        let dir = tempdir().unwrap();
        let image = dir.path().join("IMG_1.jpg");
        std::fs::write(&image, b"pretend jpeg").unwrap();

        let written = write(&image, &full()).unwrap().unwrap();
        assert_eq!(written, dir.path().join("IMG_1.jpg.xmp"));
        assert!(std::fs::read_to_string(&written)
            .unwrap()
            .contains("Munich"));
    }

    #[test]
    fn there_is_nothing_to_write_when_nothing_is_known() {
        let dir = tempdir().unwrap();
        let image = dir.path().join("IMG_1.jpg");
        std::fs::write(&image, b"pretend jpeg").unwrap();

        assert!(Sidecar::default().is_empty());
        assert!(write(&image, &Sidecar::default()).unwrap().is_none());
        assert!(!dir.path().join("IMG_1.jpg.xmp").exists());
    }

    #[test]
    fn knowing_only_one_thing_is_still_worth_a_sidecar() {
        let only_date = Sidecar {
            taken: Some(taken()),
            ..Sidecar::default()
        };
        assert!(!only_date.is_empty());
        let rendered = render(&only_date);
        assert!(rendered.contains("xmp:CreateDate"));
        assert!(!rendered.contains("GPSLatitude"));
        assert!(!rendered.contains("dc:subject"));
    }

    #[test]
    fn what_eonsort_writes_says_which_tool_wrote_it() {
        assert!(render(&full()).contains("x:xmptk=\"eonsort\""));
    }
}
