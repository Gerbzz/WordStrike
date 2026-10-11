"""Generate small self-authored OOXML and PNG acceptance inputs (development only)."""

from pathlib import Path
from struct import pack
from zipfile import ZIP_DEFLATED, ZipFile
import zlib


ROOT = Path(__file__).parent


def png_chunk(kind: bytes, data: bytes) -> bytes:
    return pack(">I", len(data)) + kind + data + pack(">I", zlib.crc32(kind + data))


def chart_png() -> bytes:
    width, height = 320, 180
    rows = bytearray()
    for y in range(height):
        rows.append(0)
        for x in range(width):
            bar = (40 <= x < 100 and 55 <= y < 155) or (125 <= x < 185 and 30 <= y < 155) or (210 <= x < 270 and 75 <= y < 155)
            rows.extend((25, 93, 186) if bar else (247, 249, 253))
    return b"\x89PNG\r\n\x1a\n" + png_chunk(b"IHDR", pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)) + png_chunk(b"IDAT", zlib.compress(bytes(rows), 9)) + png_chunk(b"IEND", b"")


CT = b'''<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Default Extension="png" ContentType="image/png"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>'''
ROOT_RELS = b'''<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>'''
DOC_RELS = b'''<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/chart.png"/>
</Relationships>'''
OPEN = '''<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
 xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
 xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
 xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
 xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:body>'''
CLOSE = '</w:body></w:document>'
IMAGE = '''<w:p><w:r><w:drawing><wp:inline><wp:docPr id="1" name="Blue chart" descr="Blue bar chart"/>
<a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed="rId5"/>
</pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>'''


def write_docx(name: str, body: str, include_image: bool) -> None:
    with ZipFile(ROOT / name, "w", ZIP_DEFLATED) as archive:
        archive.writestr("[Content_Types].xml", CT)
        archive.writestr("_rels/.rels", ROOT_RELS)
        archive.writestr("word/document.xml", (OPEN + body + CLOSE).encode())
        if include_image:
            archive.writestr("word/_rels/document.xml.rels", DOC_RELS)
        if include_image and name != "missing-image.docx":
            archive.writestr("word/media/chart.png", chart_png())


if __name__ == "__main__":
    ROOT.mkdir(parents=True, exist_ok=True)
    (ROOT / "blue-chart.png").write_bytes(chart_png())
    write_docx(
        "with-image.docx",
        '''<w:p><w:pPr><w:pStyle w:val="Title"/></w:pPr><w:r><w:t>Quarterly Field Notes</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Findings</w:t></w:r></w:p>
<w:p><w:r><w:t>Alpha </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>beta</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>First result</w:t></w:r></w:p>'''
        + IMAGE,
        True,
    )
    write_docx("tracked-change.docx", '<w:p><w:ins w:id="1"><w:r><w:t>Inserted</w:t></w:r></w:ins><w:del w:id="2"><w:r><w:delText>Deleted</w:delText></w:r></w:del></w:p>', False)
    write_docx("missing-image.docx", IMAGE, True)
