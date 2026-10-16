// Reads the semantic constraints DocumentFormat.OpenXml 3.5.1 registers on its element classes, by reflection
// over the package's assemblies, and writes them to standard output as one JSON array: each element class that
// has constraints, with its qualified name, then each constraint in registration order with its class, the
// version it applies from, the applications it applies to, and its fields.
using System.Collections;
using System.Globalization;
using System.Reflection;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;
using DocumentFormat.OpenXml;
using DocumentFormat.OpenXml.Wordprocessing;

const BindingFlags Declared = BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;

var metadata = typeof(OpenXmlElement).GetProperty("Metadata", BindingFlags.Instance | BindingFlags.NonPublic)
    ?? throw new InvalidOperationException("OpenXmlElement has no Metadata property");
var classes = new[] { typeof(OpenXmlElement).Assembly, typeof(Paragraph).Assembly }
    .Distinct()
    .SelectMany(assembly => assembly.GetTypes())
    .Where(type => type.IsClass && !type.IsAbstract && typeof(OpenXmlElement).IsAssignableFrom(type)
        && type.GetConstructor(Type.EmptyTypes) is not null)
    .OrderBy(type => type.FullName, StringComparer.Ordinal);

var records = new JsonArray();
foreach (var type in classes)
{
    var element = (OpenXmlElement)Activator.CreateInstance(type)!;
    var data = metadata.GetValue(element)!;
    var constraints = (IEnumerable)data.GetType().GetProperty("Constraints")!.GetValue(data)!;
    var rows = new JsonArray();
    foreach (var constraint in constraints)
    {
        var kind = constraint.GetType();
        var fields = new JsonObject();
        for (var level = kind; level is not null && level.Name != "SemanticConstraint"; level = level.BaseType)
        {
            foreach (var field in level.GetFields(Declared).OrderBy(field => field.MetadataToken))
            {
                fields[field.Name] = Value(field.GetValue(constraint));
            }
        }
        rows.Add(new JsonObject
        {
            ["kind"] = kind.Name,
            ["version"] = kind.GetProperty("Version")!.GetValue(constraint)!.ToString(),
            ["application"] = kind.GetProperty("Application")!.GetValue(constraint)!.ToString(),
            ["fields"] = fields,
        });
    }
    if (rows.Count > 0)
    {
        records.Add(new JsonObject
        {
            ["class"] = type.FullName,
            ["namespace"] = element.NamespaceUri,
            ["local"] = element.LocalName,
            ["constraints"] = rows,
        });
    }
}
Console.WriteLine(records.ToJsonString(new JsonSerializerOptions { WriteIndented = true }));

// A field's value as JSON: a qualified name as its namespace URI and local name, a comparer by its name, a
// regular expression by its pattern, an infinite bound as the text "Infinity" or "-Infinity".
static JsonNode? Value(object? value) => value switch
{
    null => null,
    string text => text,
    bool flag => flag,
    int number => number,
    double number when double.IsFinite(number) => number,
    double number => number.ToString(CultureInfo.InvariantCulture),
    Array items => new JsonArray(items.Cast<object?>().Select(Value).ToArray()),
    Regex pattern => pattern.ToString(),
    Type type => type.FullName,
    StringComparer comparer when comparer.Equals(StringComparer.Ordinal) => "Ordinal",
    StringComparer comparer when comparer.Equals(StringComparer.OrdinalIgnoreCase) => "OrdinalIgnoreCase",
    _ when value.GetType().Name == "OpenXmlQualifiedName" => new JsonObject
    {
        ["namespace"] = value.GetType().GetProperty("Namespace")!.GetValue(value)!.ToString(),
        ["local"] = (string)value.GetType().GetProperty("Name")!.GetValue(value)!,
    },
    _ => throw new InvalidOperationException($"no JSON form for a {value.GetType()}"),
};
