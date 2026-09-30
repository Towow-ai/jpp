"""Recompute research 18 §3.2 from published CSVs, without the private corpus."""
import csv
from itertools import dropwhile
from pathlib import Path
import re

import pytest

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT/'research/data/2026-09-27-repetition'
CATEGORY_IDS = set(range(1, 13))


def occurrence_rates(filename, expected_rows, identity):
    with (DATA/filename).open(encoding='utf-8', newline='') as source:
        reader = csv.DictReader(dropwhile(lambda line: line.startswith('#'), source))
        columns = [name for name in reader.fieldnames if name.startswith('present_')]
        assert len(columns) == 12
        assert {int(name.split('_')[1]) for name in columns} == CATEGORY_IDS
        rows = list(reader)
    assert len(rows) == expected_rows
    assert len({row[identity] for row in rows}) == expected_rows
    assert all(row[column] in {'0', '1'} for row in rows for column in columns)
    if identity == 'id':
        assert all(row['scope'] == 'file' for row in rows)
        assert all(int(row['total_lines']) >= 0 for row in rows)
        empty = [row for row in rows if int(row['total_lines']) == 0]
        assert {row['id'] for row in empty} == {'B1-11', 'B4-02', 'B6-11', 'B7-01'}
        assert all(row[column] == '0' for row in empty for column in columns)
    # Match busywork_classify.py's summary: keep all 81 rows, including four
    # zero-line records, in the presence-rate denominator.
    return {int(column.split('_')[1]): sum(int(row[column]) for row in rows) / len(rows) * 100
            for column in columns}


@pytest.fixture
def expected_comparison():
    curated = occurrence_rates('84项目-scope整文件.csv', 81, 'id')
    ecosystem = occurrence_rates('生态抽样100.csv', 100, 'repo')
    return {category: (f'{curated[category]:.1f}%', f'{ecosystem[category]:.1f}%',
                       None if category == 12 else f'{ecosystem[category] - curated[category]:+.1f}')
            for category in CATEGORY_IDS}


def comparison_table(text, not_comparable):
    parts = re.split(r'^### 3\.2[ \t]+[^\n]*\n', text, flags=re.MULTILINE)
    assert len(parts) == 2, 'Expected exactly one section 3.2'
    section = re.split(r'^#{1,3} ', parts[1], maxsplit=1, flags=re.MULTILINE)[0]
    rows = {}
    for line in section.splitlines():
        match = re.fullmatch(r'\|\s*(\d+)\s+[^|]+\|([^|]+)\|([^|]+)\|([^|]+)\|', line)
        if not match:
            continue
        category = int(match[1])
        assert category not in rows, f'Duplicate category {category}'
        cells = [cell.strip().replace('**', '').replace('−', '-') for cell in match.groups()[1:]]
        rates = [re.match(r'^(\d+\.\d%)', cell) for cell in cells[:2]]
        assert all(rates), f'Missing occurrence rate for category {category}'
        if category == 12:
            assert cells[2] == not_comparable
        rows[category] = (rates[0][1], rates[1][1], None if category == 12 else cells[2])
    assert set(rows) == CATEGORY_IDS, 'Expected each of the twelve categories'
    return rows


@pytest.mark.parametrize('suffix, not_comparable', [('', 'not comparable'), ('.zh-CN', '不可比')])
def test_published_comparison_matches_csv(expected_comparison, suffix, not_comparable):
    report = ROOT/f'research/18-repetition-in-real-jev-projects{suffix}.md'
    assert comparison_table(report.read_text(encoding='utf-8'), not_comparable) == expected_comparison


def sample_report(expected_comparison):
    rows = [f'| {category} Category | {curated} | {ecosystem} | {difference or "not comparable"} |'
            for category, (curated, ecosystem, difference) in sorted(expected_comparison.items())]
    return '### 3.2 Comparison\n\n' + '\n'.join(rows) + '\n'


def test_comparison_parser_ignores_other_sections(expected_comparison):
    unrelated = '| 10 Result feedback | 3.7% | 6.0% | +2.3 |\n'
    text = '### 3.1 Other scope\n' + unrelated + sample_report(expected_comparison)
    text += '### 3.3 Calibration\n' + unrelated
    assert comparison_table(text, 'not comparable') == expected_comparison


@pytest.mark.parametrize('category, old_rate, old_difference', [
    (1, '67.9%', '+4.1'), (3, '29.6%', '-8.6'),
    (10, '3.7%', '+2.3'), (11, '3.7%', '+10.3'),
])
def test_old_comparison_values_fail(expected_comparison, category, old_rate, old_difference):
    stale = dict(expected_comparison)
    stale[category] = (old_rate, stale[category][1], old_difference)
    with pytest.raises(AssertionError):
        assert comparison_table(sample_report(stale), 'not comparable') == expected_comparison


def test_comparison_parser_rejects_duplicate_and_comparable_calibration(expected_comparison):
    text = sample_report(expected_comparison)
    with pytest.raises(AssertionError, match='Duplicate category 10'):
        comparison_table(text + '| 10 Result feedback | 21.0% | 6.0% | -15.0 |\n', 'not comparable')
    with pytest.raises(AssertionError):
        comparison_table(text.replace('not comparable', '+28.4'), 'not comparable')
