//! Writes the files a person opens in Microsoft Office to run the spike's checks: one file per row
//! of the check table, and the files the Strict correspondences need. Each file name starts with
//! the row it belongs to.
//!
//! ```text
//! cargo run -p officekit-core --example office_checks --features strict -- notes/samples <out-dir>
//! ```

#[path = "../tests/stream_generation/generation.rs"]
mod generation;

use core::error::Error;
use std::io::{Read as _, Write as _, stdout};
use std::path::PathBuf;

use clap::Parser;
use officekit_core::docx::Docx;
use officekit_core::docx::chart::ChartRanges;
use officekit_core::docx::image::PictureFit;
use officekit_core::embedded::{self, Embedded};
use officekit_core::pptx::Pptx;
use officekit_core::xlsx::Xlsx;
use officekit_markup::Read as _;
use officekit_markup::wordprocessing::{
    DocumentProtectionTypesMember, Header, HeaderOrFooterTypeMember, TextRun, write,
};
use officekit_packaging::error::PackageError;
use officekit_packaging::image::Image;
use officekit_packaging::package::{OpenOptions, Package};
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::{RelationshipType, Target};
use officekit_parts::kinds::{PresentationKind, SpreadsheetKind, WordprocessingKind};
use officekit_parts::{connections, font_table};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Element, Node};
use officekit_xml::error::WriteError;
use officekit_xml::parse::parse;
use officekit_xml::write::{Scope, WriteOptions};
use os_shim::System as _;
use os_shim::real::RealSystem;

const SALT: [u8; 16] = [
    0x84, 0x8a, 0x8e, 0x97, 0xdf, 0x64, 0x22, 0xbf, 0xfe, 0x37, 0x48, 0x72, 0xa2, 0xf1, 0x2a, 0xed,
];
const PASSWORD: &str = "secret";

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

const CHART_PART: &str = "/word/charts/chart1.xml";
const CHART_WORKBOOK: &str = "/word/embeddings/Microsoft_Excel_Worksheet.xlsx";
const WORKBOOK_SHEET: &str = "/xl/worksheets/sheet1.xml";

const WORD_KINDS: [(WordprocessingKind, &str, &str); 4] = [
    (WordprocessingKind::Document, "document", "docx"),
    (
        WordprocessingKind::MacroEnabledDocument,
        "macro-enabled-document",
        "docm",
    ),
    (WordprocessingKind::Template, "template", "dotx"),
    (
        WordprocessingKind::MacroEnabledTemplate,
        "macro-enabled-template",
        "dotm",
    ),
];

const EXCEL_KINDS: [(SpreadsheetKind, &str, &str); 5] = [
    (SpreadsheetKind::Workbook, "workbook", "xlsx"),
    (
        SpreadsheetKind::MacroEnabledWorkbook,
        "macro-enabled-workbook",
        "xlsm",
    ),
    (SpreadsheetKind::Template, "template", "xltx"),
    (
        SpreadsheetKind::MacroEnabledTemplate,
        "macro-enabled-template",
        "xltm",
    ),
    (SpreadsheetKind::AddIn, "add-in", "xlam"),
];

const POWERPOINT_KINDS: [(PresentationKind, &str, &str); 7] = [
    (PresentationKind::Presentation, "presentation", "pptx"),
    (
        PresentationKind::MacroEnabledPresentation,
        "macro-enabled-presentation",
        "pptm",
    ),
    (PresentationKind::Slideshow, "slideshow", "ppsx"),
    (
        PresentationKind::MacroEnabledSlideshow,
        "macro-enabled-slideshow",
        "ppsm",
    ),
    (PresentationKind::Template, "template", "potx"),
    (
        PresentationKind::MacroEnabledTemplate,
        "macro-enabled-template",
        "potm",
    ),
    (PresentationKind::AddIn, "add-in", "ppam"),
];

/// Korean fonts named by the Hangul (0x81) and Johab (0x82) character sets.
const KOREAN_FONTS: &str = concat!(
    "<w:fonts xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
    "<w:font w:name=\"Malgun Gothic\"><w:charset w:val=\"81\"/><w:family w:val=\"swiss\"/><w:pitch w:val=\"variable\"/></w:font>",
    "<w:font w:name=\"Gulim\"><w:charset w:val=\"82\"/><w:family w:val=\"swiss\"/><w:pitch w:val=\"variable\"/></w:font>",
    "</w:fonts>"
);

const KOREAN_DOCUMENT: &str = concat!(
    "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>",
    "<w:p><w:r><w:rPr><w:rFonts w:ascii=\"Malgun Gothic\" w:hAnsi=\"Malgun Gothic\" w:eastAsia=\"Malgun Gothic\" w:hint=\"eastAsia\"/></w:rPr><w:t>Hangul charset 81: \u{d55c}\u{ae00}</w:t></w:r></w:p>",
    "<w:p><w:r><w:rPr><w:rFonts w:ascii=\"Gulim\" w:hAnsi=\"Gulim\" w:eastAsia=\"Gulim\" w:hint=\"eastAsia\"/></w:rPr><w:t>Johab charset 82: \u{d55c}\u{ae00}</w:t></w:r></w:p>",
    "</w:body></w:document>"
);

const FIXED_TABLE_DOCUMENT: &str = concat!(
    "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>",
    "<w:tbl><w:tblPr><w:tblW w:w=\"2880\" w:type=\"dxa\"/><w:tblLayout w:type=\"fixed\"/></w:tblPr>",
    "<w:tblGrid><w:gridCol w:w=\"1440\"/><w:gridCol w:w=\"1440\"/></w:tblGrid>",
    "<w:tr><w:tc><w:tcPr><w:tcW w:w=\"1440\" w:type=\"dxa\"/></w:tcPr><w:p><w:r><w:t>A1</w:t></w:r></w:p></w:tc>",
    "<w:tc><w:tcPr><w:tcW w:w=\"1440\" w:type=\"dxa\"/></w:tcPr><w:p><w:r><w:t>B1</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    "<w:p/></w:body></w:document>"
);

const WALL_CHART_DOCUMENT: &str = concat!(
    "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" ",
    "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ",
    "xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" ",
    "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
    "xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><w:body><w:p><w:r><w:drawing>",
    "<wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"5486400\" cy=\"3200400\"/>",
    "<wp:docPr id=\"1\" name=\"Chart 1\"/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\">",
    "<c:chart r:id=\"rId2\"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p></w:body></w:document>"
);

/// A 3-D column chart whose floor and walls each have a thickness.
const WALL_CHART: &str = concat!(
    "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart>",
    "<c:view3D><c:rotX val=\"15\"/><c:rotY val=\"20\"/><c:rAngAx val=\"1\"/></c:view3D>",
    "<c:floor><c:thickness val=\"25\"/></c:floor><c:sideWall><c:thickness val=\"25\"/></c:sideWall><c:backWall><c:thickness val=\"25\"/></c:backWall>",
    "<c:plotArea><c:layout/><c:bar3DChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/>",
    "<c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:val><c:numLit><c:ptCount val=\"3\"/>",
    "<c:pt idx=\"0\"><c:v>3</c:v></c:pt><c:pt idx=\"1\"><c:v>5</c:v></c:pt><c:pt idx=\"2\"><c:v>2</c:v></c:pt></c:numLit></c:val></c:ser>",
    "<c:gapWidth val=\"150\"/><c:axId val=\"111\"/><c:axId val=\"222\"/><c:axId val=\"0\"/></c:bar3DChart>",
    "<c:catAx><c:axId val=\"111\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:crossAx val=\"222\"/></c:catAx>",
    "<c:valAx><c:axId val=\"222\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/><c:crossAx val=\"111\"/></c:valAx>",
    "</c:plotArea><c:plotVisOnly val=\"1\"/></c:chart></c:chartSpace>"
);

/// A workbook with a text-file connection on code page 1252 and another on 932.
const TEXT_CONNECTIONS: &str = concat!(
    "<connections xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
    "<connection id=\"1\" name=\"Text 1252\" type=\"6\" refreshedVersion=\"6\" background=\"1\" saveData=\"1\" sourceFile=\"C:\\okit35\\text-1252.txt\">",
    "<textPr codePage=\"1252\" sourceFile=\"C:\\okit35\\text-1252.txt\"><textFields count=\"1\"><textField/></textFields></textPr></connection>",
    "<connection id=\"2\" name=\"Text 932\" type=\"6\" refreshedVersion=\"6\" background=\"1\" saveData=\"1\" sourceFile=\"C:\\okit35\\text-932.txt\">",
    "<textPr codePage=\"932\" sourceFile=\"C:\\okit35\\text-932.txt\"><textFields count=\"1\"><textField/></textFields></textPr></connection>",
    "</connections>"
);

type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// Write the files a person opens in Microsoft Office.
#[derive(Parser)]
struct Arguments {
    /// The folder the files are written into.
    #[arg(index = 2)]
    output: PathBuf,
    /// The folder of sample files, `notes/samples`.
    #[arg(index = 1)]
    samples: PathBuf,
}

struct Checks {
    output: PathBuf,
    samples: PathBuf,
    system: RealSystem,
}

impl Checks {
    fn chart_workbook_with(docx: &mut Docx, rows: &str) -> Outcome {
        let workbook_part = PartName::new(CHART_WORKBOOK)?;
        let Embedded::Xlsx(mut xlsx) =
            embedded::open(docx.package(), &workbook_part, OpenOptions::default())?
        else {
            return Err("the chart's embedded document is not a workbook".into());
        };
        let sheet = PartName::new(WORKBOOK_SHEET)?;
        let xml = String::from_utf8(xlsx.package().part_bytes(&sheet)?)?;
        let edited = replace_rows(&xml, rows)?;
        let content_type = xlsx
            .package()
            .content_type(&sheet)
            .ok_or("the sheet has no content type")?
            .to_owned();
        xlsx.package_mut()
            .set_part_bytes(&sheet, &content_type, edited.as_bytes())?;
        embedded::store_transitional(docx.package_mut(), &workbook_part, &Embedded::Xlsx(xlsx))?;
        Ok(())
    }

    fn copy_sample(&self, sample: &str, name: &str) -> Outcome {
        self.system
            .copy(&self.samples.join(sample), &self.output.join(name))?;
        Self::report(name)
    }

    fn docx(&self, sample: &str) -> Outcome<Docx> {
        Ok(Docx::open(
            &self.system,
            &self.samples.join(sample),
            OpenOptions::default(),
        )?)
    }

    fn header(text: &str) -> Outcome<Header> {
        let xml = format!("<w:hdr xmlns:w=\"{W}\"><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:hdr>");
        Ok(Header::read(&parse(xml.as_bytes())?.root))
    }

    fn import(&self, target: &str, source: &str, name: &str) -> Outcome {
        let mut docx = self.docx(target)?;
        let source_docx = self.docx(source)?;
        let mut document = docx.main_document()?;
        let blocks = docx.import_body(&mut document, &source_docx)?;
        document
            .body
            .get_or_insert_with(Default::default)
            .content
            .extend(blocks);
        docx.set_main_document(&document)?;
        self.save(&docx, name)
    }

    fn report(name: &str) -> Outcome {
        writeln!(stdout().lock(), "{name}")?;
        Ok(())
    }

    fn row1(&self) -> Outcome {
        self.copy_sample("pagare.docx", "row1-word-saved-sample.docx")?;
        self.copy_sample(
            "Cruce base 41 MM pagado y objetado.xlsx",
            "row1-excel-saved-sample.xlsx",
        )?;
        self.copy_sample(
            "Aptoria_Pitch_Deck.pptx",
            "row1-powerpoint-saved-sample.pptx",
        )
    }

    fn row2(&self) -> Outcome {
        let new = Docx::new(WordprocessingKind::Document)?;
        new.save_strict(&self.system, &self.output.join("row2-new-strict.docx"))?;
        Self::report("row2-new-strict.docx")?;
        let mut sample = self.docx("pagare.docx")?;
        let name = "row2-sample-strict-without-unsupported-content.docx";
        loop {
            match sample.save_strict(&self.system, &self.output.join(name)) {
                Ok(()) => break,
                Err(PackageError::Write {
                    error: WriteError::NotInClass { tag, class },
                    part,
                }) => {
                    if class != ConformanceClass::Strict {
                        return Err("a Strict save refused content for another class".into());
                    }
                    let local = tag.split('/').next().unwrap_or(tag);
                    let removed = remove_named(sample.package_mut(), &part, local)?;
                    writeln!(
                        stdout().lock(),
                        "  removed {removed} {local} element(s) from {part}"
                    )?;
                    if removed == 0 {
                        return Err(format!("{local} is not an element of {part}").into());
                    }
                }
                Err(other) => return Err(other.into()),
            }
        }
        Self::report(name)
    }

    fn row3(&self) -> Outcome {
        let mut docx = self.docx("pagare.docx")?;
        let names = docx.package().part_names()?;
        for name in names {
            let Some(content_type) = docx.package().content_type(&name).map(str::to_owned) else {
                continue;
            };
            if content_type.ends_with("+xml") {
                let tree = docx.package().load_tree(&name, OpenOptions::default())?;
                docx.package_mut().store_tree(
                    &name,
                    &content_type,
                    &tree,
                    WriteOptions::default(),
                )?;
            }
        }
        self.save(&docx, "row3-namespaces-cleaned.docx")
    }

    fn row4(&self) -> Outcome {
        let mut docx = self.docx("soc_csirt_informe_mensual_threat_monitoring.docx")?;
        let rows = concat!(
            "<row r=\"2\"><c r=\"B2\"><v>42</v></c><c r=\"C2\"><v>17</v></c></row>",
            "<row r=\"3\"/><row r=\"4\"/><row r=\"5\"/>"
        );
        Self::chart_workbook_with(&mut docx, rows)?;
        self.save(&docx, "row4-embedded-workbook-edited.docx")
    }

    fn row5(&self) -> Outcome {
        for (kind, label, extension) in WORD_KINDS {
            let name = format!("row5-new-word-{label}.{extension}");
            Docx::create(&self.system, &self.output.join(&name), kind)?;
            Self::report(&name)?;
        }
        for (kind, label, extension) in EXCEL_KINDS {
            let name = format!("row5-new-excel-{label}.{extension}");
            Xlsx::create(&self.system, &self.output.join(&name), kind)?;
            Self::report(&name)?;
        }
        for (kind, label, extension) in POWERPOINT_KINDS {
            let name = format!("row5-new-powerpoint-{label}.{extension}");
            Pptx::create(&self.system, &self.output.join(&name), kind)?;
            Self::report(&name)?;
        }
        Ok(())
    }

    fn row6_charts(&self) -> Outcome {
        let mut docx = self.docx("soc_csirt_informe_mensual_threat_monitoring.docx")?;
        let rows = concat!(
            "<row r=\"2\"><c r=\"A2\" t=\"inlineStr\"><is><t>Week 1</t></is></c><c r=\"B2\"><v>12</v></c><c r=\"C2\"><v>3</v></c></row>",
            "<row r=\"3\"><c r=\"A3\" t=\"inlineStr\"><is><t>Week 2</t></is></c><c r=\"B3\"><v>18</v></c><c r=\"C3\"><v>5</v></c></row>",
            "<row r=\"4\"><c r=\"A4\" t=\"inlineStr\"><is><t>Week 3</t></is></c><c r=\"B4\"><v>9</v></c><c r=\"C4\"><v>1</v></c></row>",
            "<row r=\"5\"><c r=\"A5\" t=\"inlineStr\"><is><t>Week 4</t></is></c><c r=\"B5\"><v>15</v></c><c r=\"C5\"><v>4</v></c></row>"
        );
        Self::chart_workbook_with(&mut docx, rows)?;
        docx.refresh_chart_caches(&PartName::new(CHART_PART)?, ChartRanges::ToLastRow)?;
        self.save(&docx, "row6-chart-refreshed.docx")
    }

    fn row6_headers(&self) -> Outcome {
        let mut plain = self.docx("contrato_garantia_hipotecaria.docx")?;
        plain.add_header(
            0,
            HeaderOrFooterTypeMember::Default,
            &Self::header("Header added, default")?,
        )?;
        self.save(&plain, "row6-header-added-default.docx")?;
        let mut more = self.docx("contrato_garantia_hipotecaria.docx")?;
        more.add_header(
            0,
            HeaderOrFooterTypeMember::First,
            &Self::header("Header added, first page")?,
        )?;
        more.add_header(
            0,
            HeaderOrFooterTypeMember::Even,
            &Self::header("Header added, even pages")?,
        )?;
        self.save(&more, "row6-header-added-first-and-even.docx")?;
        let mut replaced = self.docx("pagare.docx")?;
        replaced.add_header(
            0,
            HeaderOrFooterTypeMember::Default,
            &Self::header("Header replaced")?,
        )?;
        self.save(&replaced, "row6-header-replaced.docx")?;
        let mut removed = self.docx("pagare.docx")?;
        removed.remove_header(0, HeaderOrFooterTypeMember::Default)?;
        self.save(&removed, "row6-header-removed.docx")
    }

    fn row6_imports(&self) -> Outcome {
        self.import(
            "pagare.docx",
            "notes_source.docx",
            "row6-import-notes-source-into-pagare.docx",
        )?;
        self.import(
            "pagare.docx",
            "formulario_cumplimiento_fatca.docx",
            "row6-import-fatca-into-pagare.docx",
        )?;
        self.import(
            "formulario_cumplimiento_fatca.docx",
            "canvas.instructure.docx",
            "row6-import-canvas-into-fatca.docx",
        )?;
        self.import(
            "soc_csirt_informe_mensual_threat_monitoring.docx",
            "soc_csirt_informe_mensual_threat_monitoring.docx",
            "row6-import-soc-into-itself.docx",
        )?;
        self.import(
            "formulario_cumplimiento_fatca.docx",
            "formulario_cumplimiento_fatca.docx",
            "row6and8-import-fatca-into-itself.docx",
        )?;
        self.import(
            "pagare.docx",
            "pagare.docx",
            "row6and8-import-pagare-into-itself.docx",
        )
    }

    fn row6_pictures(&self) -> Outcome {
        let mut bytes = Vec::new();
        self.system
            .open(&self.samples.join("wide-300x100.png"))?
            .read_to_end(&mut bytes)?;
        let image = Image::from_bytes(bytes)?;
        for (fit, name) in [
            (PictureFit::Frame, "row6-picture-fit-frame.docx"),
            (PictureFit::Width, "row6-picture-fit-width.docx"),
        ] {
            let mut docx = self.docx("pagare.docx")?;
            let main = docx.main_document_part().clone();
            let mut tree = docx.package().load_tree(&main, OpenOptions::default())?;
            let declarations = tree.root.namespace_declarations.clone();
            let element = picture_run(&mut tree.root).ok_or("pagare holds no picture")?;
            let mut run = TextRun::read(element);
            let replaced = docx.set_picture(&main, &mut run, &image, fit)?;
            let mut scope = Scope::default();
            scope.start(&declarations);
            *element = write::text_run(&run, &mut scope);
            let content_type = docx
                .package()
                .content_type(&main)
                .ok_or("the main part has no content type")?
                .to_owned();
            docx.package_mut()
                .store_tree(&main, &content_type, &tree, WriteOptions::default())?;
            if let Some(id) = replaced {
                let text = String::from_utf8(docx.package().part_bytes(&main)?)?;
                if !text.contains(&format!("\"{id}\"")) {
                    docx.package_mut().remove_relationship(Some(&main), &id)?;
                }
            }
            self.save(&docx, name)?;
        }
        Ok(())
    }

    fn row6_protections(&self) -> Outcome {
        let mut transitional = self.docx("pagare.docx")?;
        transitional.protect(
            DocumentProtectionTypesMember::ReadOnly,
            PASSWORD,
            &SALT,
            ConformanceClass::Transitional,
        )?;
        self.save(&transitional, "row6and7-protect-transitional-pagare.docx")?;
        let mut strict = Docx::new(WordprocessingKind::Document)?;
        strict.protect(
            DocumentProtectionTypesMember::ReadOnly,
            PASSWORD,
            &SALT,
            ConformanceClass::Strict,
        )?;
        let name = "row6and7-protect-strict-new.docx";
        strict.save_strict(&self.system, &self.output.join(name))?;
        Self::report(name)
    }

    fn row6_stream(&self) -> Outcome {
        let name = "row6-stream-1000-rows-data-descriptors.docx";
        let file = self.system.create(&self.output.join(name))?;
        generation::generate(&generation::template()?, 1000, file)?.flush()?;
        Self::report(name)
    }

    fn save(&self, docx: &Docx, name: &str) -> Outcome {
        let warnings = docx.save_transitional(&self.system, &self.output.join(name))?;
        if !warnings.is_empty() {
            writeln!(stdout().lock(), "  {} save warning(s)", warnings.len())?;
        }
        Self::report(name)
    }

    fn strict_correspondences(&self) -> Outcome {
        let mut korean = Docx::new(WordprocessingKind::Document)?;
        let main = korean.main_document_part().clone();
        let content_type = korean.kind().main_content_type();
        korean
            .package_mut()
            .set_part_bytes(&main, content_type, KOREAN_DOCUMENT.as_bytes())?;
        let fonts = PartName::new("/word/fontTable.xml")?;
        korean.package_mut().set_part_bytes(
            &fonts,
            font_table::CONTENT_TYPE,
            KOREAN_FONTS.as_bytes(),
        )?;
        korean.package_mut().relate(
            Some(&main),
            &fonts,
            RelationshipType::FONT_TABLE.transitional,
        )?;
        self.save(&korean, "strictA-korean-charset-81-82.docx")?;

        let mut walls = Docx::new(WordprocessingKind::Document)?;
        let walls_main = walls.main_document_part().clone();
        walls.package_mut().set_part_bytes(
            &walls_main,
            WordprocessingKind::Document.main_content_type(),
            WALL_CHART_DOCUMENT.as_bytes(),
        )?;
        let chart = PartName::new(CHART_PART)?;
        walls.package_mut().set_part_bytes(
            &chart,
            "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
            WALL_CHART.as_bytes(),
        )?;
        walls.package_mut().add_relationship(
            Some(&walls_main),
            "rId2",
            &Target::Part(chart),
            RelationshipType::CHART.transitional,
        )?;
        self.save(&walls, "strictB-chart-wall-thickness.docx")?;

        let mut table = Docx::new(WordprocessingKind::Document)?;
        let table_main = table.main_document_part().clone();
        table.package_mut().set_part_bytes(
            &table_main,
            WordprocessingKind::Document.main_content_type(),
            FIXED_TABLE_DOCUMENT.as_bytes(),
        )?;
        self.save(&table, "strictD-table-fixed-width.docx")?;

        let mut book = Xlsx::new(SpreadsheetKind::Workbook)?;
        let book_main = book.workbook_part().clone();
        let connections_part = PartName::new("/xl/connections.xml")?;
        book.package_mut().set_part_bytes(
            &connections_part,
            connections::CONTENT_TYPE,
            TEXT_CONNECTIONS.as_bytes(),
        )?;
        book.package_mut().relate(
            Some(&book_main),
            &connections_part,
            connections::RELATIONSHIP_TYPE.transitional,
        )?;
        let book_name = "strictC-text-file-connections-1252-932.xlsx";
        book.save_transitional(&self.system, &self.output.join(book_name))?;
        Self::report(book_name)
    }
}

fn main() -> Outcome {
    let arguments = Arguments::parse();
    let checks = Checks {
        output: arguments.output,
        samples: arguments.samples,
        system: RealSystem::new(),
    };
    checks.system.create_dir_all(&checks.output)?;
    checks.row1()?;
    checks.row2()?;
    checks.row3()?;
    checks.row4()?;
    checks.row5()?;
    checks.row6_headers()?;
    checks.row6_imports()?;
    checks.row6_pictures()?;
    checks.row6_charts()?;
    checks.row6_protections()?;
    checks.row6_stream()?;
    checks.strict_correspondences()
}

fn has_blip(element: &Element) -> bool {
    element.name.local == "blip" || element.child_elements().any(has_blip)
}

/// The first `w:r`, in document order, that holds an `a:blip`.
fn picture_run(element: &mut Element) -> Option<&mut Element> {
    if element.name.local == "r" && element.name.prefix == "w" && has_blip(element) {
        return Some(element);
    }
    element.children.iter_mut().find_map(|node| {
        if let Node::Element(child) = node {
            picture_run(child)
        } else {
            None
        }
    })
}

/// Every element named `local` below `element`, removed; how many.
fn remove_in(element: &mut Element, local: &str) -> usize {
    let before = element.children.len();
    element
        .children
        .retain(|node| !matches!(node, Node::Element(child) if child.name.local == local));
    let mut removed = before - element.children.len();
    for node in &mut element.children {
        if let Node::Element(child) = node {
            removed += remove_in(child, local);
        }
    }
    removed
}

/// Every element named `local` of the part `name`, removed; how many.
fn remove_named(package: &mut Package, name: &PartName, local: &str) -> Outcome<usize> {
    let content_type = package
        .content_type(name)
        .ok_or("the part has no content type")?
        .to_owned();
    let mut tree = package.load_tree(name, OpenOptions::default())?;
    let removed = remove_in(&mut tree.root, local);
    package.store_tree(name, &content_type, &tree, WriteOptions::default())?;
    Ok(removed)
}

/// `sheet` with its rows from row 2 on replaced by `rows`.
fn replace_rows(sheet: &str, rows: &str) -> Outcome<String> {
    let start = sheet.find("<row r=\"2\"").ok_or("the sheet has no row 2")?;
    let end = sheet
        .find("</sheetData>")
        .ok_or("the sheet has no sheetData end")?;
    Ok(format!("{}{rows}{}", &sheet[..start], &sheet[end..]))
}
