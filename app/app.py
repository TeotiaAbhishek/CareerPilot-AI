import html
import re
import sys
from pathlib import Path

import streamlit as st


# ============================================================
# PROJECT SETUP
# ============================================================

PROJECT_ROOT = Path(__file__).resolve().parent.parent
SRC_PATH = PROJECT_ROOT / "src"

sys.path.append(str(SRC_PATH))

from database import (
    get_kept_jobs,
    get_job_documents,
    mark_job_applied,
)

from document_generator import (
    generate_resume_markdown,
    generate_cover_letter_markdown,
)

from google_drive import (
    publish_document,
)


# ============================================================
# PAGE CONFIG
# ============================================================

st.set_page_config(
    page_title="CareerPilot AI",
    layout="wide",
    initial_sidebar_state="collapsed",
)


# ============================================================
# CSS
# ============================================================

CSS = """
<style>

/* ==========================================================
   THEME
========================================================== */

:root {
    --cp-blue: #4c8df6;
    --cp-blue-strong: #3277e6;
    --cp-blue-soft: rgba(76, 141, 246, 0.11);

    --cp-green: #48d49d;
    --cp-green-soft: rgba(72, 212, 157, 0.09);

    --cp-amber: #f2b94b;
    --cp-amber-soft: rgba(242, 185, 75, 0.09);

    --cp-border: rgba(145, 150, 160, 0.26);
    --cp-border-strong: rgba(145, 150, 160, 0.42);

    --cp-muted: rgba(190, 195, 205, 0.76);

    --cp-card: rgba(255, 255, 255, 0.018);
    --cp-card-hover: rgba(76, 141, 246, 0.055);
}


/* ==========================================================
   STREAMLIT TOP BAR
========================================================== */

header[data-testid="stHeader"] {
    height: 2.35rem;
    background: transparent;
}


/* ==========================================================
   PAGE
========================================================== */

.block-container {
    max-width: 1800px !important;

    padding-top: 2.45rem !important;
    padding-bottom: 0.35rem !important;

    padding-left: 1rem !important;
    padding-right: 1rem !important;
}


[data-testid="stVerticalBlock"] {
    gap: 0.48rem !important;
}


/* ==========================================================
   CAREERPILOT HEADER
========================================================== */

.cp-header {
    min-height: 52px;

    display: flex;
    align-items: center;
    justify-content: space-between;

    padding:
        0
        0.15rem
        0.72rem
        0.15rem;

    margin-bottom: 0.72rem;

    border-bottom:
        1px solid var(--cp-border);
}


.cp-brand {
    font-size: 1.68rem;

    font-weight: 780;

    line-height: 1.05;

    letter-spacing: -0.025em;
}



/* ==========================================================
   SEARCH
========================================================== */

.st-key-job_search input {
    min-height: 40px !important;

    border-radius: 8px !important;

    font-size: 0.82rem !important;
}


.cp-results-count {
    padding:
        0.12rem
        0.08rem
        0.35rem
        0.08rem;

    color: var(--cp-muted);

    font-size: 0.77rem;

    font-weight: 650;
}


/* ==========================================================
   SCROLLBAR APPEARANCE
========================================================== */

.st-key-job_list,
.st-key-job_details {
    scrollbar-width: thin;
    scrollbar-color:
        rgba(145, 150, 160, 0.38)
        transparent;
}


.st-key-job_list::-webkit-scrollbar,
.st-key-job_details::-webkit-scrollbar {
    width: 6px;
}


.st-key-job_list::-webkit-scrollbar-track,
.st-key-job_details::-webkit-scrollbar-track {
    background: transparent;
}


.st-key-job_list::-webkit-scrollbar-thumb,
.st-key-job_details::-webkit-scrollbar-thumb {
    background:
        rgba(145, 150, 160, 0.38);

    border-radius: 50px;
}


/* ==========================================================
   JOB CARD SPACING
========================================================== */

.st-key-job_list div.stButton {
    margin-bottom: 0.62rem !important;
}


/* ==========================================================
   JOB CARD
========================================================== */

.st-key-job_list div.stButton > button {
    width: 100% !important;

    height: auto !important;
    min-height: 0 !important;

    padding:
        0.88rem
        0.95rem !important;

    border-radius: 10px !important;

    border:
        1px solid var(--cp-border)
        !important;

    background:
        var(--cp-card)
        !important;

    box-shadow: none !important;

    white-space: normal !important;

    justify-content: flex-start !important;
    align-items: flex-start !important;

    text-align: left !important;

    transition:
        border-color 0.15s ease,
        background-color 0.15s ease,
        transform 0.15s ease;
}


.st-key-job_list div.stButton > button:hover {
    background:
        var(--cp-card-hover)
        !important;

    border-color:
        rgba(76, 141, 246, 0.62)
        !important;

    transform: translateY(-1px);
}


/* ==========================================================
   FORCE EVERY CARD TO ALIGN IDENTICALLY

   This fixes the random indentation seen on some jobs.
========================================================== */

.st-key-job_list div.stButton > button *,
.st-key-job_list
div.stButton
[data-testid="stMarkdownContainer"] {

    width: 100% !important;

    text-align: left !important;

    justify-content: flex-start !important;

    margin-left: 0 !important;

    padding-left: 0 !important;

    text-indent: 0 !important;
}


.st-key-job_list
div.stButton
[data-testid="stMarkdownContainer"] p {

    margin:
        0
        0
        0.28rem
        0 !important;

    text-align: left !important;
}


/* ==========================================================
   CARD NORMAL TEXT
========================================================== */

.st-key-job_list div.stButton > button p {
    color: inherit !important;

    font-size: 0.76rem !important;

    line-height: 1.37 !important;
}


/* ==========================================================
   CARD JOB TITLE
========================================================== */

.st-key-job_list div.stButton > button strong {
    font-size: 0.90rem !important;

    line-height: 1.25 !important;

    font-weight: 760 !important;
}


/* ==========================================================
   CARD COMPANY
========================================================== */

.st-key-job_list div.stButton > button em {
    color: var(--cp-muted) !important;

    font-size: 0.78rem !important;

    font-style: normal !important;

    font-weight: 620 !important;
}


/* ==========================================================
   SCORE BAR — LAST LINE OF EACH CARD
========================================================== */

.st-key-job_list
div.stButton
> button
p:last-child {

    color: #8fc2ff !important;

    font-size: 0.77rem !important;

    font-weight: 760 !important;

    letter-spacing: 0.01em !important;

    margin-top: 0.24rem !important;
}


/* ==========================================================
   SELECTED JOB CARD
========================================================== */

.st-key-selected_job_card {
    margin: 0 !important;
    padding: 0 !important;
}


.st-key-selected_job_card div.stButton > button {

    background:
        rgba(76, 141, 246, 0.10)
        !important;

    border:
        1.5px solid #4c8df6
        !important;

    box-shadow:
        inset 3px 0 0 #4c8df6,
        0 0 0 1px rgba(76, 141, 246, 0.08)
        !important;
}


.st-key-selected_job_card div.stButton > button:hover {

    background:
        rgba(76, 141, 246, 0.13)
        !important;

    border-color:
        #4c8df6
        !important;
}

.st-key-job_list
button[kind="primary"]:hover,
.st-key-job_list
button[data-testid="stBaseButton-primary"]:hover {

    background:
        rgba(76, 141, 246, 0.15)
        !important;

    border-color:
        var(--cp-blue)
        !important;
}


/* ==========================================================
   RIGHT JOB HEADER
========================================================== */

.cp-job-title {
    margin-bottom: 0.36rem;

    font-size: 1.55rem;

    font-weight: 770;

    line-height: 1.18;

    letter-spacing: -0.018em;
}


.cp-company {
    margin-bottom: 0.62rem;

    font-size: 1.02rem;

    font-weight: 650;
}


/* ==========================================================
   META
========================================================== */

.cp-meta {
    display: flex;

    flex-wrap: wrap;

    gap: 0.38rem;

    margin-bottom: 0.75rem;
}


.cp-chip {
    display: inline-flex;

    align-items: center;

    padding:
        0.24rem
        0.52rem;

    border:
        1px solid
        rgba(150, 150, 160, 0.21);

    border-radius: 6px;

    background:
        rgba(255, 255, 255, 0.025);

    color: var(--cp-muted);

    font-size: 0.73rem;
}


/* ==========================================================
   RIGHT-SIDE MATCH SCORE
========================================================== */

.cp-match-header {
    display: flex;

    align-items: center;

    justify-content: space-between;

    gap: 1rem;

    margin:
        0.35rem
        0
        0.25rem
        0;
}


.cp-match-label {
    font-size: 0.81rem;

    font-weight: 720;
}


.cp-match-number {
    color: #7aacf8;

    font-size: 0.82rem;

    font-weight: 760;
}


.cp-match-track {
    width: 100%;

    height: 7px;

    overflow: hidden;

    border-radius: 999px;

    background:
        rgba(150, 150, 160, 0.16);

    margin-bottom: 0.75rem;
}


.cp-match-fill {
    height: 100%;

    border-radius: 999px;

    background:
        linear-gradient(
            90deg,
            #4388ef,
            #52c7aa
        );
}


/* ==========================================================
   ACTIONS
========================================================== */

.st-key-job_actions {
    padding:
        0.35rem
        0
        0.80rem
        0;

    margin-bottom: 0.25rem;

    border-bottom:
        1px solid var(--cp-border);
}


.st-key-job_actions button,
.st-key-job_actions a {
    min-height: 40px !important;

    border-radius: 8px !important;

    font-size: 0.81rem !important;

    font-weight: 620 !important;
}


/* ==========================================================
   SECTION TITLE
========================================================== */

.cp-section-title {
    margin-top: 0.95rem;

    margin-bottom: 0.50rem;

    font-size: 1.02rem;

    font-weight: 730;
}


/* ==========================================================
   SUMMARY
========================================================== */

.cp-summary {
    padding:
        0.72rem
        0.84rem;

    margin-bottom: 0.75rem;

    border:
        1px solid
        rgba(76, 141, 246, 0.20);

    border-radius: 9px;

    background:
        rgba(76, 141, 246, 0.045);

    font-size: 0.83rem;

    line-height: 1.50;
}


/* ==========================================================
   MATCHED REQUIREMENTS
========================================================== */

.st-key-matched_requirements {
    padding:
        0.52rem
        0.72rem
        0.62rem
        0.72rem;

    border-left:
        3px solid var(--cp-green);

    border-radius: 9px;

    background:
        var(--cp-green-soft);
}


/* ==========================================================
   MISSING REQUIREMENTS
========================================================== */

.st-key-missing_requirements {
    padding:
        0.52rem
        0.72rem
        0.62rem
        0.72rem;

    border-left:
        3px solid var(--cp-amber);

    border-radius: 9px;

    background:
        var(--cp-amber-soft);
}


.st-key-matched_requirements li,
.st-key-missing_requirements li {
    margin-bottom: 0.22rem;

    font-size: 0.80rem !important;

    line-height: 1.48 !important;
}


/* ==========================================================
   DIVIDERS
========================================================== */

hr {
    margin:
        0.90rem
        0 !important;

    opacity: 0.30;
}


/* ==========================================================
   DESKTOP

   The native Streamlit container creates scrolling.
   These selectors resize BOTH the keyed container and its
   actual stVerticalBlockBorderWrapper to browser height.
========================================================== */

@media (min-width: 901px) {

    html,
    body,
    [data-testid="stAppViewContainer"],
    [data-testid="stMain"] {

        height: 100dvh !important;

        overflow: hidden !important;
    }


    .block-container {

        height: 100dvh !important;

        overflow: hidden !important;
    }


    /* ---------------- LEFT ---------------- */

    div.st-key-job_list,
    div[data-testid="stVerticalBlockBorderWrapper"].st-key-job_list,
    .st-key-job_list
    > div[data-testid="stVerticalBlockBorderWrapper"] {

        height:
            calc(100dvh - 184px)
            !important;

        max-height:
            calc(100dvh - 184px)
            !important;

        min-height: 340px !important;

        overflow-y: auto !important;

        overflow-x: hidden !important;

        overscroll-behavior: contain;
    }


    /* ---------------- RIGHT ---------------- */

    div.st-key-job_details,
    div[data-testid="stVerticalBlockBorderWrapper"].st-key-job_details,
    .st-key-job_details
    > div[data-testid="stVerticalBlockBorderWrapper"] {

        height:
            calc(100dvh - 112px)
            !important;

        max-height:
            calc(100dvh - 112px)
            !important;

        min-height: 420px !important;

        overflow-y: auto !important;

        overflow-x: hidden !important;

        overscroll-behavior: contain;
    }


    .st-key-job_details {
        padding-right: 0.55rem;
    }

}


/* ==========================================================
   PHONE / SMALL WINDOWS

   Streamlit stacks columns automatically.
   Give each pane its own sensible scroll area.
========================================================== */

@media (max-width: 900px) {

    html,
    body,
    [data-testid="stAppViewContainer"],
    [data-testid="stMain"] {

        height: auto !important;

        overflow: auto !important;
    }


    .block-container {

        height: auto !important;

        overflow: visible !important;

        padding-left: 0.70rem !important;

        padding-right: 0.70rem !important;
    }


    .cp-brand {
        font-size: 1.42rem;
    }


    div.st-key-job_list,
    div[data-testid="stVerticalBlockBorderWrapper"].st-key-job_list,
    .st-key-job_list
    > div[data-testid="stVerticalBlockBorderWrapper"] {

        height: 48dvh !important;

        max-height: 48dvh !important;

        min-height: 300px !important;

        overflow-y: auto !important;
    }


    div.st-key-job_details,
    div[data-testid="stVerticalBlockBorderWrapper"].st-key-job_details,
    .st-key-job_details
    > div[data-testid="stVerticalBlockBorderWrapper"] {

        height: 72dvh !important;

        max-height: 72dvh !important;

        min-height: 430px !important;

        overflow-y: auto !important;
    }


    .cp-job-title {
        font-size: 1.28rem;
    }

}


/* ==========================================================
   FOOTER
========================================================== */

footer {
    visibility: hidden;
}

</style>
"""

st.html(CSS)


# ============================================================
# LOAD DATA
# ============================================================

@st.cache_data(
    ttl=60,
    show_spinner=False,
)
def load_jobs():
    return get_kept_jobs()


jobs = load_jobs()


if not jobs:
    st.info(
        "No relevant jobs available."
    )
    st.stop()


# ============================================================
# HELPERS
# ============================================================

def clean_text(value):
    """
    Normalises whitespace from scraped SEEK values.

    This prevents one job card from randomly appearing
    indented differently from another.
    """

    if value is None:
        return ""

    return " ".join(
        str(value).split()
    )


def markdown_escape(value):

    value = clean_text(value)

    return re.sub(
        r"([\\`*_{}\[\]#>|])",
        r"\\\1",
        value,
    )


def html_escape(value):
    return html.escape(
        clean_text(value)
    )


def score_bar(score, segments=10):
    score = max(
        0,
        min(
            100,
            int(score),
        ),
    )

    filled = round(
        (score / 100) * segments
    )

    empty = segments - filled

    return (
        "▰" * filled
        + "▱" * empty
    )


def get_selected_job(all_jobs):

    selected_id = (
        st.session_state.get(
            "selected_job_id"
        )
    )

    for job in all_jobs:

        if (
            job["job_id"]
            == selected_id
        ):
            return job

    return all_jobs[0]


def build_job_card(job):

    title = markdown_escape(
        job.get("title")
        or "Untitled role"
    )

    company = markdown_escape(
        job.get("company")
        or "Unknown company"
    )

    location = markdown_escape(
        job.get("location")
        or "Location not specified"
    )

    work_type = markdown_escape(
        job.get("work_type")
        or "Work type not specified"
    )

    short_description = markdown_escape(
        job.get(
            "short_description"
        )
        or ""
    )

    posted = markdown_escape(
        job.get("posted")
        or ""
    )

    score = int(
        job["relevance_score"]
    )


    if len(short_description) > 120:

        short_description = (
            short_description[:117]
            + "..."
        )


    label = (
        f"**{title}**\n\n"
        f"*{company}*\n\n"
        f"📍 {location} · {work_type}"
    )


    if short_description:

        label += (
            f"\n\n"
            f"{short_description}"
        )


    label += (
        f"\n\n"
        f"**{score}% match**   "
        f"{score_bar(score)}"
    )


    if posted:

        label += (
            f" · {posted}"
        )


    return label

def create_document(
    job_id,
    document_type,
    regenerate=False,
):
    """
    Generate/publish a document.

    Cost-saving behaviour:
    - Existing Markdown + missing Drive file -> reuse Markdown
    - No Markdown -> call OpenAI
    - regenerate=True -> always call OpenAI again
    """

    documents = (
        get_job_documents(job_id)
        or {}
    )

    if document_type == "resume":

        existing_markdown = (
            documents.get(
                "resume_markdown"
            )
        )

        if (
            regenerate
            or not existing_markdown
        ):

            generate_resume_markdown(
                job_id
            )

    elif document_type == "cover":

        existing_markdown = (
            documents.get(
                "cover_letter_markdown"
            )
        )

        if (
            regenerate
            or not existing_markdown
        ):

            generate_cover_letter_markdown(
                job_id
            )

    else:

        raise ValueError(
            "document_type must be "
            "'resume' or 'cover'"
        )

    return publish_document(
        job_id=job_id,
        document_type=document_type,
    )
# ============================================================
# SELECT DEFAULT JOB
# ============================================================

if (
    "selected_job_id"
    not in st.session_state
):

    st.session_state.selected_job_id = (
        jobs[0]["job_id"]
    )


# ============================================================
# HEADER
#
# Kept deliberately as ONE unindented HTML string so
# Markdown can never interpret it as a code block.
# ============================================================

st.html(
    '<div class="cp-header">'
    '<div>'
    '<div class="cp-brand">CareerPilot AI</div>'
    '</div>'
    '</div>'
)


# ============================================================
# MAIN LAYOUT
# ============================================================

left_panel, right_panel = st.columns(
    [0.34, 0.66],
    gap="medium",
)


# ============================================================
# LEFT PANEL
# ============================================================

with left_panel:

    # --------------------------------------------------------
    # LIVE SEARCH
    # --------------------------------------------------------

    search_query = st.text_input(
        "Search jobs",
        placeholder=(
            "Search title, company or location..."
        ),
        label_visibility="collapsed",
        key="job_search",
        type="search",
        live="250ms",
    )


    query = clean_text(
        search_query
    ).lower()


    if query:

        filtered_jobs = [

            job

            for job in jobs

            if query in (
                clean_text(
                    job.get("title")
                )
                + " "
                + clean_text(
                    job.get("company")
                )
                + " "
                + clean_text(
                    job.get("location")
                )
            ).lower()
        ]

    else:

        filtered_jobs = jobs


    st.html(
        (
            '<div class="cp-results-count">'
            f'{len(filtered_jobs)} relevant jobs'
            '</div>'
        )
    )


    # --------------------------------------------------------
    # FILTER CAN CHANGE CURRENT SELECTION
    # --------------------------------------------------------

    if filtered_jobs:

        visible_ids = {
            job["job_id"]
            for job in filtered_jobs
        }


        if (
            st.session_state.selected_job_id
            not in visible_ids
        ):

            st.session_state.selected_job_id = (
                filtered_jobs[0]["job_id"]
            )


    # --------------------------------------------------------
    # LEFT SCROLL AREA
    #
    # 1200 is only a fallback.
    # CSS overrides it to viewport height on desktop/mobile.
    # --------------------------------------------------------

    with st.container(
        height=1200,
        border=False,
        key="job_list",
        autoscroll=False,
    ):

        if not filtered_jobs:

            st.caption(
                "No jobs match your search."
            )


        for job in filtered_jobs:

            selected = (
                job["job_id"]
                == st.session_state.selected_job_id
            )


            if selected:

                with st.container(
                    key="selected_job_card"
                ):

                    clicked = st.button(
                        build_job_card(job),
                        key=(
                            f'job_'
                            f'{job["job_id"]}'
                        ),
                        use_container_width=True,
                    )

            else:

                clicked = st.button(
                    build_job_card(job),
                    key=(
                        f'job_'
                        f'{job["job_id"]}'
                    ),
                    use_container_width=True,
                )


            if clicked:

                st.session_state.selected_job_id = (
                    job["job_id"]
                )

                st.rerun()


# ============================================================
# SELECTED JOB
# ============================================================

selected_job = get_selected_job(
    jobs
)


# ============================================================
# RIGHT PANEL
# ============================================================

with right_panel:

    # --------------------------------------------------------
    # RIGHT SCROLL AREA
    #
    # Same fallback approach as left panel.
    # --------------------------------------------------------

    with st.container(
        height=1200,
        border=False,
        key="job_details",
        autoscroll=False,
    ):

        # ====================================================
        # VALUES
        # ====================================================

        title = html_escape(
            selected_job.get("title")
            or "Untitled role"
        )

        company = html_escape(
            selected_job.get("company")
            or "Unknown company"
        )

        location = html_escape(
            selected_job.get("location")
            or "Location not specified"
        )

        work_type = html_escape(
            selected_job.get("work_type")
            or "Work type not specified"
        )

        classification = html_escape(
            selected_job.get(
                "classification"
            )
        )

        posted = html_escape(
            selected_job.get("posted")
        )

        score = int(
            selected_job[
                "relevance_score"
            ]
        )


        # ====================================================
        # JOB HEADER
        # ====================================================

        st.html(
            (
                f'<div class="cp-job-title">'
                f'{title}'
                f'</div>'

                f'<div class="cp-company">'
                f'{company}'
                f'</div>'
            )
        )


        # ====================================================
        # META CHIPS
        # ====================================================

        metadata = [
            location,
            work_type,
        ]


        if classification:

            metadata.append(
                classification
            )


        if posted:

            metadata.append(
                f"Posted {posted}"
            )


        chips = "".join(

            (
                '<span class="cp-chip">'
                f'{item}'
                '</span>'
            )

            for item in metadata
        )


        st.html(
            (
                '<div class="cp-meta">'
                f'{chips}'
                '</div>'
            )
        )


        # ====================================================
        # MATCH SCORE
        # ====================================================

        st.html(
            (
                '<div class="cp-match-header">'

                '<span class="cp-match-label">'
                'Profile relevance'
                '</span>'

                '<span class="cp-match-number">'
                f'{score}% match'
                '</span>'

                '</div>'

                '<div class="cp-match-track">'

                '<div class="cp-match-fill" '
                f'style="width:{score}%">'
                '</div>'

                '</div>'
            )
        )


        # ====================================================
        # ACTIONS
        # ====================================================

        documents = (
            get_job_documents(
                selected_job["job_id"]
            )
            or {}
        )
        resume_ready = bool(
            documents.get("resume_doc_url")
            and documents.get("resume_pdf_url")
        )
        cover_ready = bool(
            documents.get("cover_letter_doc_url")
            and documents.get("cover_letter_pdf_url")
        )
        with st.container(
            key="job_actions"
        ):
            resume_col, cover_col, apply_col = (
                st.columns(
                    [1, 1, 0.72],
                    gap="small",
                )
            )
            # =================================================
            # RESUME
            # =================================================
            with resume_col:
            
                if resume_ready:
                
                    open_col, regen_col = (
                        st.columns(
                            [5, 1],
                            gap="small",
                        )
                    )
                    with open_col:
                    
                        st.link_button(
                            "📄 Open resume",
                            documents[
                                "resume_doc_url"
                            ],
                            use_container_width=True,
                        )
                    with regen_col:
                    
                        regenerate_resume = (
                            st.button(
                                "↻",
                                key=(
                                    "regenerate_resume_"
                                    f'{selected_job["job_id"]}'
                                ),
                                help="Regenerate resume",
                                use_container_width=True,
                            )
                        )
                    if regenerate_resume:
                    
                        try:
                        
                            with st.spinner(
                                "Regenerating resume..."
                            ):
                                create_document(
                                    job_id=(
                                        selected_job[
                                            "job_id"
                                        ]
                                    ),
                                    document_type="resume",
                                    regenerate=True,
                                )
                            st.success(
                                "Resume regenerated."
                            )
                            st.rerun()
                        except Exception as exc:
                        
                            st.error(
                                f"Resume generation failed: "
                                f"{exc}"
                            )
                else:
                
                    generate_resume = st.button(
                        "📄 Generate resume",
                        key=(
                            "generate_resume_"
                            f'{selected_job["job_id"]}'
                        ),
                        use_container_width=True,
                    )
                    if generate_resume:
                    
                        try:
                        
                            with st.spinner(
                                "Creating tailored resume..."
                            ):
                                create_document(
                                    job_id=(
                                        selected_job[
                                            "job_id"
                                        ]
                                    ),
                                    document_type="resume",
                                )
                            st.success(
                                "Resume created."
                            )
                            st.rerun()
                        except Exception as exc:
                        
                            st.error(
                                f"Resume generation failed: "
                                f"{exc}"
                            )
            # =================================================
            # COVER LETTER
            # =================================================
            with cover_col:
            
                if cover_ready:
                
                    open_col, regen_col = (
                        st.columns(
                            [5, 1],
                            gap="small",
                        )
                    )
                    with open_col:
                    
                        st.link_button(
                            "✉️ Open cover letter",
                            documents[
                                "cover_letter_doc_url"
                            ],
                            use_container_width=True,
                        )
                    with regen_col:
                    
                        regenerate_cover = (
                            st.button(
                                "↻",
                                key=(
                                    "regenerate_cover_"
                                    f'{selected_job["job_id"]}'
                                ),
                                help=(
                                    "Regenerate cover letter"
                                ),
                                use_container_width=True,
                            )
                        )
                    if regenerate_cover:
                    
                        try:
                        
                            with st.spinner(
                                "Regenerating cover letter..."
                            ):
                                create_document(
                                    job_id=(
                                        selected_job[
                                            "job_id"
                                        ]
                                    ),
                                    document_type="cover",
                                    regenerate=True,
                                )
                            st.success(
                                "Cover letter regenerated."
                            )
                            st.rerun()
                        except Exception as exc:
                        
                            st.error(
                                "Cover letter generation "
                                f"failed: {exc}"
                            )
                else:
                
                    generate_cover = st.button(
                        "✉️ Generate cover letter",
                        key=(
                            "generate_cover_"
                            f'{selected_job["job_id"]}'
                        ),
                        use_container_width=True,
                    )
                    if generate_cover:
                    
                        try:
                        
                            with st.spinner(
                                "Creating cover letter..."
                            ):
                                create_document(
                                    job_id=(
                                        selected_job[
                                            "job_id"
                                        ]
                                    ),
                                    document_type="cover",
                                )
                            st.success(
                                "Cover letter created."
                            )
                            st.rerun()
                        except Exception as exc:
                        
                            st.error(
                                "Cover letter generation "
                                f"failed: {exc}"
                            )
            # =================================================
            # APPLY
            # =================================================
            with apply_col:
            
                if selected_job.get(
                    "apply_url"
                ):
                    st.link_button(
                        "Apply ↗",
                        selected_job[
                            "apply_url"
                        ],
                        use_container_width=True,
                        type="primary",
                    )

                if st.button(
                    "✓ Mark applied",
                    key=(
                        "mark_applied_"
                        f'{selected_job["job_id"]}'
                    ),
                    use_container_width=True,
                ):

                    mark_job_applied(
                        selected_job[
                            "job_id"
                        ]
                    )

                    load_jobs.clear()

                    st.rerun()


        # ====================================================
        # ANALYSIS
        # ====================================================

        analysis = (
            selected_job.get(
                "analysis"
            )
            or {}
        )


        st.html(
            '<div class="cp-section-title">'
            'Why this job matches'
            '</div>'
        )


        summary = html_escape(
            analysis.get(
                "summary"
            )
            or
            "No analysis summary available."
        )


        st.html(
            (
                '<div class="cp-summary">'
                f'{summary}'
                '</div>'
            )
        )


        # ====================================================
        # MATCHED / MISSING
        # ====================================================

        matched_col, missing_col = (
            st.columns(
                2,
                gap="medium",
            )
        )


        # ----------------------------------------------------
        # MATCHED
        # ----------------------------------------------------

        with matched_col:

            with st.container(
                key="matched_requirements"
            ):

                st.markdown(
                    "### ✅ Matched requirements"
                )


                matched = (
                    analysis.get(
                        "matched_requirements",
                        []
                    )
                )


                if matched:

                    for requirement in matched:

                        st.markdown(
                            f"- {requirement}"
                        )

                else:

                    st.caption(
                        "No major matches identified."
                    )


        # ----------------------------------------------------
        # MISSING
        # ----------------------------------------------------

        with missing_col:

            with st.container(
                key="missing_requirements"
            ):

                st.markdown(
                    "### ⚠️ Missing requirements"
                )


                missing = (
                    analysis.get(
                        "missing_requirements",
                        []
                    )
                )


                if missing:

                    for requirement in missing:

                        st.markdown(
                            f"- {requirement}"
                        )

                else:

                    st.caption(
                        "No major gaps identified."
                    )


        # ====================================================
        # UNKNOWN
        # ====================================================

        unknown = (
            analysis.get(
                "unknown_requirements",
                []
            )
        )


        if unknown:

            with st.expander(
                "❓ Requirements to verify"
            ):

                for requirement in unknown:

                    st.markdown(
                        f"- {requirement}"
                    )


        # ====================================================
        # JOB DESCRIPTION
        # ====================================================

        st.divider()


        st.html(
            '<div class="cp-section-title">'
            'Job description'
            '</div>'
        )


        st.markdown(
            selected_job.get(
                "description"
            )
            or
            "No job description available."
        )