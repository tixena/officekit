// The parity oracle (D25): validates each file named on the command line with DocumentFormat.OpenXml 3.5.1 at every
// target of D23 and writes one JSON array of error records to standard output, in the record shape officekit's
// `validate --json` writes, with SDK 3.5.1's message key in `id` where officekit writes its own `code`.
using System.Text.Json;
using System.Text.Json.Nodes;
using DocumentFormat.OpenXml;
using DocumentFormat.OpenXml.Packaging;
using DocumentFormat.OpenXml.Validation;

var targets = new[]
{
    ("Office2010", FileFormatVersions.Office2010), ("Office2013", FileFormatVersions.Office2013), ("Office2016", FileFormatVersions.Office2016),
    ("Office2019", FileFormatVersions.Office2019), ("Office2021", FileFormatVersions.Office2021), ("Microsoft365", FileFormatVersions.Microsoft365),
};
var records = new JsonArray();
foreach (var file in args)
{
    foreach (var (target, version) in targets)
    {
        using OpenXmlPackage package = Path.GetExtension(file).ToLowerInvariant() switch
        {
            ".xlsx" or ".xlsm" or ".xltx" or ".xltm" or ".xlam" => SpreadsheetDocument.Open(file, false),
            ".pptx" or ".pptm" or ".potx" or ".potm" or ".ppsx" or ".ppsm" or ".ppam" => PresentationDocument.Open(file, false),
            _ => WordprocessingDocument.Open(file, false),
        };
        // MaxNumberOfErrors 0 reports every error.
        foreach (var error in new OpenXmlValidator(version) { MaxNumberOfErrors = 0 }.Validate(package))
        {
            records.Add(new JsonObject
            {
                ["file"] = Path.GetFileName(file),
                ["target"] = target,
                ["kind"] = error.ErrorType switch
                {
                    ValidationErrorType.Schema => "schema",
                    ValidationErrorType.Semantic => "semantic",
                    ValidationErrorType.Package => "package",
                    _ => "markup-compatibility",
                },
                ["id"] = error.Id,
                ["part"] = error.Part?.Uri.OriginalString,
                ["related_part"] = error.RelatedPart?.Uri.OriginalString,
                ["path"] = Steps(error.Path),
                ["message"] = error.Description,
            });
        }
    }
}
Console.WriteLine(records.ToJsonString(new JsonSerializerOptions { WriteIndented = false }));

// `/w:document[1]/w:body[1]` becomes [{namespace, local, index}, ...], each prefix resolved through the path's own
// namespace table, so a path compares by namespace URI and not by the prefix the file chose.
static JsonNode? Steps(XmlPath? path)
{
    if (path is null || string.IsNullOrEmpty(path.XPath)) return null;
    var steps = new JsonArray();
    foreach (var step in path.XPath.Split('/', StringSplitOptions.RemoveEmptyEntries))
    {
        var open = step.LastIndexOf('[');
        var name = step[..open];
        var index = int.Parse(step[(open + 1)..^1]);
        var colon = name.LastIndexOf(':');
        var (qualifier, local) = colon < 0 ? ("", name) : (name[..colon], name[(colon + 1)..]);
        var uri = qualifier.Contains("://") ? qualifier : path.Namespaces.LookupNamespace(qualifier) ?? "";
        steps.Add(new JsonObject { ["namespace"] = uri, ["local"] = local, ["index"] = index });
    }
    return steps;
}
