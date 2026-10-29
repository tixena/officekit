use std::path::Path;

use os_shim::mock::MemorySystem;

use super::SdkData;

const NAMESPACES: &str = r#"[{"Prefix": "", "Uri": ""}]"#;

const ATTRIBUTE_ROW: &str = r#"
[[attribute]]
type = "x:CT_Inner/x:inner"
replaces = "x:val"
qname = ":val"
"#;

const CHART: &str = r#"{
  "TargetNamespace": "chart",
  "Types": [
    {"Name": "c:CT_List/c:extLst", "Children": [{"Name": "x:CT_Wrong/c:ext"}], "Particle": {"Kind": "Sequence", "Items": [{"Name": "x:CT_Wrong/c:ext", "Occurs": [{}], "InitialVersion": "Office2019"}]}},
    {"Name": "x:CT_Wrong/c:ext", "Version": "Office2019", "Children": [{"Name": "x:CT_Inner/x:inner"}]},
    {"Name": "x:CT_Inner/x:inner", "Version": "Office2019", "Attributes": [{"QName": "x:val", "Type": "BooleanValue"}]},
    {"Name": "c:CT_Other/c:ext", "Attributes": [{"QName": ":uri", "Type": "StringValue"}], "Particle": {"Kind": "Choice", "Items": [{"Name": "x:CT_Inner/x:inner", "InitialVersion": "Office2013"}, {"Kind": "Any", "Occurs": [{"Max": 1}]}]}}
  ]
}"#;

const ROW: &str = r#"
[[extension]]
list = "c:CT_List/c:extLst"
replaces = "x:CT_Wrong/c:ext"
type = "c:CT_Right/c:ext"
attributes_of = "c:CT_Other/c:ext"
children = ["x:CT_Inner/x:inner"]
"#;

fn read() -> SdkData {
    let system = MemorySystem::new()
        .with_file("data/namespaces.json", NAMESPACES.as_bytes())
        .unwrap()
        .with_file("data/typed/namespaces.json", b"[]")
        .unwrap()
        .with_file("data/schemas/chart.json", CHART.as_bytes())
        .unwrap()
        .with_file("data/parts/README.md", b"no parts")
        .unwrap()
        .with_file("data/schematrons.json", b"[]")
        .unwrap();
    SdkData::read(&system, Path::new("data")).unwrap()
}

#[test]
fn a_row_replaces_the_type_and_points_the_list_at_the_new_one() {
    let mut data = read();
    data.correct(ROW).unwrap();

    assert!(!data.types.contains_key("x:CT_Wrong/c:ext"));
    let list = &data.types["c:CT_List/c:extLst"];
    assert_eq!(list.children, ["c:CT_Right/c:ext"]);
    let item = &list.particle.as_ref().unwrap().items[0];
    assert_eq!(item.name.as_deref(), Some("c:CT_Right/c:ext"));
    assert_eq!(item.initial_version, None);

    let right = &data.types["c:CT_Right/c:ext"];
    assert_eq!(right.attributes, data.types["c:CT_Other/c:ext"].attributes);
    assert_eq!(right.children, ["x:CT_Inner/x:inner"]);
    assert_eq!(right.version, None);
    let items = &right.particle.as_ref().unwrap().items;
    assert_eq!(items[0].name.as_deref(), Some("x:CT_Inner/x:inner"));
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].initial_version, None);
}

#[test]
fn a_row_naming_a_type_the_data_lacks_stops_the_generator() {
    for (from, to) in [
        ("x:CT_Wrong/c:ext", "x:CT_Missing/c:ext"),
        ("c:CT_List/c:extLst", "c:CT_Missing/c:extLst"),
        ("c:CT_Other/c:ext", "c:CT_Missing/c:ext"),
        ("x:CT_Inner/x:inner", "x:CT_Missing/x:inner"),
    ] {
        let mut data = read();
        let error = data.correct(&ROW.replace(from, to)).unwrap_err();
        assert!(format!("{error:#}").contains(to), "{error:#}");
    }
}

#[test]
fn a_row_whose_list_does_not_hold_the_type_stops_the_generator() {
    let mut data = read();
    let row = ROW.replace("c:CT_List/c:extLst", "c:CT_Other/c:ext");
    let error = data.correct(&row).unwrap_err();
    assert!(format!("{error:#}").contains("does not hold"), "{error:#}");
}

#[test]
fn a_row_missing_a_field_stops_the_generator() {
    let mut data = read();
    let error = data
        .correct("[[extension]]\nlist = \"c:CT_List/c:extLst\"")
        .unwrap_err();
    assert!(format!("{error:#}").contains("has no"), "{error:#}");
}

#[test]
fn a_row_renames_the_attribute_it_names() {
    let mut data = read();
    data.correct(ATTRIBUTE_ROW).unwrap();
    assert_eq!(data.types["x:CT_Inner/x:inner"].attributes[0].qname, ":val");
}

#[test]
fn an_attribute_row_naming_what_the_data_lacks_stops_the_generator() {
    for (from, to) in [
        ("x:CT_Inner/x:inner", "x:CT_Missing/x:inner"),
        ("x:val", "x:missing"),
    ] {
        let mut data = read();
        let error = data.correct(&ATTRIBUTE_ROW.replace(from, to)).unwrap_err();
        assert!(format!("{error:#}").contains(to), "{error:#}");
    }
}
