import re
from io import BytesIO

from docx import Document
from docx.enum.text import (
    WD_ALIGN_PARAGRAPH,
    WD_LINE_SPACING,
)
from docx.oxml import OxmlElement
from docx.oxml.ns import qn
from docx.opc.constants import RELATIONSHIP_TYPE
from docx.shared import Inches, Pt, RGBColor


# ============================================================
# DESIGN
# ============================================================

FONT_NAME = "Arial"

TEXT_COLOR = "202124"
MUTED_COLOR = "5F6368"

# Professional dark blue
ACCENT_COLOR = "285A8E"

# Light divider
BORDER_COLOR = "D9DEE5"


# ============================================================
# CANDIDATE DETAILS
#
# Used for the cover-letter header.
# Resume already receives these through Markdown.
# ============================================================

CANDIDATE_NAME = "Abhishek Teotia"
CANDIDATE_LOCATION = "Sydney, NSW, Australia"
CANDIDATE_PHONE = "[removed]"
CANDIDATE_EMAIL = "[removed]"

LINKEDIN_URL = (
    "https://www.linkedin.com/in/"
    "teotia-abhishek/"
)

GITHUB_URL = (
    "https://github.com/"
    "TeotiaAbhishek"
)

HACKERRANK_URL = (
    "https://www.hackerrank.com/profile/"
    "abhishek2486teo1"
)


# ============================================================
# BASIC FONT HELPERS
# ============================================================

def set_run_font(
    run,
    size=9.4,
    bold=False,
    color=TEXT_COLOR,
    italic=False,
):
    run.font.name = FONT_NAME
    run.font.size = Pt(size)
    run.font.bold = bold
    run.font.italic = italic
    run.font.color.rgb = RGBColor.from_string(
        color
    )


def set_paragraph_spacing(
    paragraph,
    before=0,
    after=0,
    line_spacing=1.0,
):
    fmt = paragraph.paragraph_format

    fmt.space_before = Pt(before)
    fmt.space_after = Pt(after)

    fmt.line_spacing_rule = (
        WD_LINE_SPACING.SINGLE
    )

    fmt.line_spacing = line_spacing


# ============================================================
# PARAGRAPH BORDER
# ============================================================

def add_bottom_border(
    paragraph,
    color=BORDER_COLOR,
    size=6,
    space=3,
):
    """
    Adds a subtle horizontal line below a paragraph.
    """

    p = paragraph._p
    pPr = p.get_or_add_pPr()

    pBdr = pPr.find(
        qn("w:pBdr")
    )

    if pBdr is None:
        pBdr = OxmlElement(
            "w:pBdr"
        )
        pPr.append(pBdr)

    bottom = OxmlElement(
        "w:bottom"
    )

    bottom.set(
        qn("w:val"),
        "single",
    )

    bottom.set(
        qn("w:sz"),
        str(size),
    )

    bottom.set(
        qn("w:space"),
        str(space),
    )

    bottom.set(
        qn("w:color"),
        color,
    )

    pBdr.append(bottom)


# ============================================================
# HYPERLINK
# ============================================================

def add_hyperlink(
    paragraph,
    text,
    url,
    size=9.0,
):
    """
    Creates a real clickable hyperlink while displaying
    only the clean label, e.g. LinkedIn instead of the URL.
    """

    part = paragraph.part

    relationship_id = (
        part.relate_to(
            url,
            RELATIONSHIP_TYPE.HYPERLINK,
            is_external=True,
        )
    )

    hyperlink = OxmlElement(
        "w:hyperlink"
    )

    hyperlink.set(
        qn("r:id"),
        relationship_id,
    )


    run = OxmlElement(
        "w:r"
    )

    run_properties = OxmlElement(
        "w:rPr"
    )


    # Font
    fonts = OxmlElement(
        "w:rFonts"
    )

    fonts.set(
        qn("w:ascii"),
        FONT_NAME,
    )

    fonts.set(
        qn("w:hAnsi"),
        FONT_NAME,
    )

    run_properties.append(
        fonts
    )


    # Font size
    size_element = OxmlElement(
        "w:sz"
    )

    size_element.set(
        qn("w:val"),
        str(int(size * 2)),
    )

    run_properties.append(
        size_element
    )


    # Hyperlink colour
    color = OxmlElement(
        "w:color"
    )

    color.set(
        qn("w:val"),
        ACCENT_COLOR,
    )

    run_properties.append(
        color
    )


    # No underline
    underline = OxmlElement(
        "w:u"
    )

    underline.set(
        qn("w:val"),
        "none",
    )

    run_properties.append(
        underline
    )


    run.append(
        run_properties
    )


    text_element = OxmlElement(
        "w:t"
    )

    text_element.text = text

    run.append(
        text_element
    )

    hyperlink.append(
        run
    )

    paragraph._p.append(
        hyperlink
    )


# ============================================================
# INLINE MARKDOWN PARSER
# ============================================================

def add_inline_markdown(
    paragraph,
    text,
    size=9.4,
    color=TEXT_COLOR,
):
    """
    Supports:

    **bold**
    [LinkedIn](https://...)

    while keeping normal text untouched.
    """

    pattern = re.compile(
        r"(\*\*.*?\*\*"
        r"|\[[^\]]+\]\([^)]+\))"
    )

    parts = pattern.split(
        text
    )


    for part in parts:

        if not part:
            continue


        # ----------------------------------------------------
        # BOLD
        # ----------------------------------------------------

        if (
            part.startswith("**")
            and part.endswith("**")
        ):

            value = part[2:-2]

            run = paragraph.add_run(
                value
            )

            set_run_font(
                run,
                size=size,
                bold=True,
                color=color,
            )

            continue


        # ----------------------------------------------------
        # LINK
        # ----------------------------------------------------

        link_match = re.fullmatch(
            r"\[([^\]]+)\]\(([^)]+)\)",
            part,
        )

        if link_match:

            label = (
                link_match.group(1)
            )

            url = (
                link_match.group(2)
            )

            add_hyperlink(
                paragraph,
                label,
                url,
                size=size,
            )

            continue


        # ----------------------------------------------------
        # NORMAL TEXT
        # ----------------------------------------------------

        run = paragraph.add_run(
            part
        )

        set_run_font(
            run,
            size=size,
            color=color,
        )


# ============================================================
# RESUME FORMATTER
# ============================================================

def markdown_to_resume_docx(
    markdown,
):
    document = Document()


    # ========================================================
    # PAGE
    # ========================================================

    section = document.sections[0]

    section.top_margin = Inches(0.42)
    section.bottom_margin = Inches(0.42)

    section.left_margin = Inches(0.52)
    section.right_margin = Inches(0.52)


    # ========================================================
    # DEFAULT STYLE
    # ========================================================

    normal = document.styles["Normal"]

    normal.font.name = FONT_NAME
    normal.font.size = Pt(9.25)


    # ========================================================
    # MARKDOWN
    # ========================================================

    lines = markdown.splitlines()

    in_header = True

    last_header_paragraph = None


    for raw_line in lines:

        line = raw_line.strip()


        if not line:
            continue


        # ====================================================
        # NAME
        # ====================================================

        if line.startswith("# "):

            name = line[2:].strip()

            paragraph = (
                document.add_paragraph()
            )

            paragraph.alignment = (
                WD_ALIGN_PARAGRAPH.CENTER
            )

            run = paragraph.add_run(
                name
            )

            set_run_font(
                run,
                size=18.5,
                bold=True,
                color=ACCENT_COLOR,
            )

            set_paragraph_spacing(
                paragraph,
                after=1,
            )

            last_header_paragraph = (
                paragraph
            )

            continue


        # ====================================================
        # SECTION HEADING
        # ====================================================

        if line.startswith("## "):

            # The contact/header area is now finished
            if in_header:
                in_header = False


            heading = line[3:].strip()

            paragraph = (
                document.add_paragraph()
            )

            paragraph.paragraph_format.keep_with_next = (
                True
            )

            run = paragraph.add_run(
                heading
            )

            set_run_font(
                run,
                size=11.2,
                bold=True,
                color=ACCENT_COLOR,
            )

            set_paragraph_spacing(
                paragraph,
                before=5,
                after=2,
            )

            add_bottom_border(
                paragraph,
                color=ACCENT_COLOR,
                size=4,
                space=2,
            )

            continue


        # ====================================================
        # COMPANY / PROJECT / UNIVERSITY
        # ====================================================

        if line.startswith("### "):

            text = line[4:].strip()

            paragraph = (
                document.add_paragraph()
            )

            paragraph.paragraph_format.keep_with_next = (
                True
            )

            run = paragraph.add_run(
                text
            )

            set_run_font(
                run,
                size=10.2,
                bold=True,
            )

            set_paragraph_spacing(
                paragraph,
                before=2.8,
                after=0.3,
            )

            continue


        # ====================================================
        # BULLETS
        # ====================================================

        if line.startswith("- "):

            text = line[2:].strip()

            paragraph = (
                document.add_paragraph()
            )

            fmt = (
                paragraph.paragraph_format
            )

            fmt.left_indent = Inches(
                0.16
            )

            fmt.first_line_indent = Inches(
                -0.11
            )


            bullet_run = (
                paragraph.add_run(
                    "•  "
                )
            )

            set_run_font(
                bullet_run,
                size=9.4,
                color=ACCENT_COLOR,
            )


            add_inline_markdown(
                paragraph,
                text,
                size=9.4,
            )


            set_paragraph_spacing(
                paragraph,
                after=0.7,
                line_spacing=1.0,
            )

            continue


        # ====================================================
        # HEADER LINES
        #
        # Everything before Professional Summary / first
        # ## section is centred.
        # ====================================================

        if in_header:

            paragraph = (
                document.add_paragraph()
            )

            paragraph.alignment = (
                WD_ALIGN_PARAGRAPH.CENTER
            )


            add_inline_markdown(
                paragraph,
                line,
                size=9.6,
                color=MUTED_COLOR,
            )


            set_paragraph_spacing(
                paragraph,
                after=0.2,
            )


            last_header_paragraph = (
                paragraph
            )

            continue


        # ====================================================
        # NORMAL BODY LINE
        # ====================================================

        paragraph = (
            document.add_paragraph()
        )


        add_inline_markdown(
            paragraph,
            line,
            size=9.6,
        )


        set_paragraph_spacing(
            paragraph,
            after=1.1,
            line_spacing=1.0,
        )


    # ========================================================
    # SAVE TO MEMORY
    # ========================================================

    buffer = BytesIO()

    document.save(
        buffer
    )

    buffer.seek(0)

    return buffer


# ============================================================
# COVER LETTER HEADER
# ============================================================

def add_cover_letter_header(
    document,
):

    # --------------------------------------------------------
    # NAME
    # --------------------------------------------------------

    paragraph = document.add_paragraph()

    paragraph.alignment = (
        WD_ALIGN_PARAGRAPH.CENTER
    )

    run = paragraph.add_run(
        CANDIDATE_NAME
    )

    set_run_font(
        run,
        size=16.5,
        bold=True,
        color=ACCENT_COLOR,
    )

    set_paragraph_spacing(
        paragraph,
        after=1,
    )


    # --------------------------------------------------------
    # CONTACT
    # --------------------------------------------------------

    paragraph = document.add_paragraph()

    paragraph.alignment = (
        WD_ALIGN_PARAGRAPH.CENTER
    )

    contact = (
        f"{CANDIDATE_LOCATION}"
        f" | {CANDIDATE_PHONE}"
        f" | {CANDIDATE_EMAIL}"
    )

    run = paragraph.add_run(
        contact
    )

    set_run_font(
        run,
        size=9.0,
        color=MUTED_COLOR,
    )

    set_paragraph_spacing(
        paragraph,
        after=1,
    )


    # --------------------------------------------------------
    # SOCIAL LINKS
    # --------------------------------------------------------

    paragraph = document.add_paragraph()

    paragraph.alignment = (
        WD_ALIGN_PARAGRAPH.CENTER
    )


    add_hyperlink(
        paragraph,
        "LinkedIn",
        LINKEDIN_URL,
        size=9.0,
    )


    run = paragraph.add_run(
        "   |   "
    )

    set_run_font(
        run,
        size=9.0,
        color=MUTED_COLOR,
    )


    add_hyperlink(
        paragraph,
        "GitHub",
        GITHUB_URL,
        size=9.0,
    )


    run = paragraph.add_run(
        "   |   "
    )

    set_run_font(
        run,
        size=9.0,
        color=MUTED_COLOR,
    )


    add_hyperlink(
        paragraph,
        "HackerRank",
        HACKERRANK_URL,
        size=9.0,
    )


    set_paragraph_spacing(
        paragraph,
        after=6,
    )


    add_bottom_border(
        paragraph,
        color=BORDER_COLOR,
        size=5,
        space=5,
    )


# ============================================================
# COVER LETTER FORMATTER
# ============================================================

def markdown_to_cover_letter_docx(
    markdown,
):
    document = Document()


    # ========================================================
    # PAGE
    # ========================================================

    section = document.sections[0]

    section.top_margin = Inches(0.60)
    section.bottom_margin = Inches(0.60)

    section.left_margin = Inches(0.75)
    section.right_margin = Inches(0.75)


    # ========================================================
    # DEFAULT
    # ========================================================

    normal = document.styles["Normal"]

    normal.font.name = FONT_NAME
    normal.font.size = Pt(11)


    # ========================================================
    # CONTENT
    # ========================================================

    lines = markdown.splitlines()


    for raw_line in lines:

        line = raw_line.strip()


        if not line:
            continue


        # We don't need a giant "Cover Letter" title.
        if line.lower() in {
            "# cover letter",
            "## cover letter",
        }:
            continue


        paragraph = (
            document.add_paragraph()
        )


        # ----------------------------------------------------
        # GREETING
        # ----------------------------------------------------

        if line.startswith("Dear "):

            add_inline_markdown(
                paragraph,
                line,
                size=11.0,           
            )

            set_paragraph_spacing(
                paragraph,
                after=8,
            )

            continue


        # ----------------------------------------------------
        # SIGN-OFF
        # ----------------------------------------------------

        if (
            line.startswith(
                "Kind regards"
            )
            or line.startswith(
                "Sincerely"
            )
        ):

            add_inline_markdown(
                paragraph,
                line,
                size=11.0,
            )

            set_paragraph_spacing(
                paragraph,
                before=4,
                after=2,
            )

            continue


        # ----------------------------------------------------
        # NAME
        # ----------------------------------------------------

        if line == CANDIDATE_NAME:

            run = paragraph.add_run(
                line
            )

            set_run_font(
                run,
                size=11.0,
                bold=True,
                color=ACCENT_COLOR,
            )

            set_paragraph_spacing(
                paragraph,
                after=0,
            )

            continue


        # ----------------------------------------------------
        # BODY PARAGRAPH
        # ----------------------------------------------------

        add_inline_markdown(
            paragraph,
            line,
            size=11.0,
        )

        set_paragraph_spacing(
            paragraph,
            after=8,
            line_spacing=1.12,
        )


    # ========================================================
    # SAVE
    # ========================================================

    buffer = BytesIO()

    document.save(
        buffer
    )

    buffer.seek(0)

    return buffer