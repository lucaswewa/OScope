// OScope's standard report (P47): a title, its metadata, its sections of
// text, images, tables and charts, and its provenance. The renderer puts
// the report at /report.json, its images at /images/ and its charts, as
// SVG, at /charts/.
#let report = json("/report.json")

#set document(title: report.title)
#set page(
  paper: "a4",
  margin: (x: 2cm, y: 2.2cm),
  footer: context align(center, text(8pt, counter(page).display("1 / 1", both: true))),
)
#set text(font: "Roboto", size: 10pt, lang: "en")
#show heading.where(level: 1): set text(13pt)
#show figure.caption: set text(9pt, style: "italic")

#align(center)[
  #text(18pt, weight: "bold", report.title)
  #if report.subtitle != none [ \ #text(12pt, report.subtitle) ]
  \ #text(9pt, fill: gray, report.date)
]

#let pairs(fields) = fields.map(f => (strong(f.name), f.value)).flatten()

#if report.metadata.len() > 0 {
  table(columns: (auto, 1fr), stroke: none, ..pairs(report.metadata))
}

#for section in report.sections [
  = #section.heading
  #for block in section.blocks {
    if block.kind == "text" {
      par(block.text)
    } else if block.kind == "image" {
      let width = if block.width == none { 80% } else { block.width * 100% }
      figure(image("/images/" + block.name, width: width), caption: block.caption)
    } else if block.kind == "table" {
      let head = block.columns.map(c => strong(c))
      figure(
        table(columns: block.columns.len(), ..head, ..block.rows.flatten()),
        caption: block.caption,
      )
    } else if block.kind == "chart" {
      figure(image("/charts/" + block.file, width: 90%), caption: block.caption)
    }
  }
]

#if report.provenance.len() > 0 [
  = Provenance
  #table(columns: (auto, 1fr), stroke: none, ..pairs(report.provenance))
]
