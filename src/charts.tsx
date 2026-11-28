import { useEffect, useRef } from "react";
import { init, use } from "echarts/core";
import type { ComposeOption, EChartsType } from "echarts/core";
import { BarChart, LineChart, PieChart } from "echarts/charts";
import type { BarSeriesOption, LineSeriesOption, PieSeriesOption } from "echarts/charts";
import { GridComponent, TooltipComponent } from "echarts/components";
import type { GridComponentOption, TooltipComponentOption } from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import { compact, number, percent, quarterLabel, sectorName } from "./format";
import { useLocale } from "./locale";
import type { HistoryPoint, Sector, Snapshot } from "./api";

use([BarChart, LineChart, PieChart, GridComponent, TooltipComponent, CanvasRenderer]);

type ChartOption = ComposeOption<
  BarSeriesOption | LineSeriesOption | PieSeriesOption | GridComponentOption | TooltipComponentOption
>;

const colors = ["#6258ee", "#8679f5", "#b0a6fa", "#d3cafd", "#a4acd4", "#e9eaf4"];
const tooltip: TooltipComponentOption = {
  confine: true,
  renderMode: "richText",
  backgroundColor: "#1b223b",
  borderWidth: 0,
  textStyle: { color: "#fff", fontSize: 12 },
  padding: [9, 12],
};

function Chart({ option, label }: { option: ChartOption; label: string }) {
  const node = useRef<HTMLDivElement>(null);
  const instance = useRef<EChartsType | null>(null);

  useEffect(() => {
    const element = node.current;
    if (!element) return;
    const chart = init(element, undefined, { renderer: "canvas" });
    instance.current = chart;
    const observer = new ResizeObserver(() => chart.resize());
    observer.observe(element);
    return () => {
      observer.disconnect();
      chart.dispose();
      instance.current = null;
    };
  }, []);

  useEffect(() => {
    instance.current?.setOption(option, { notMerge: true });
  }, [option]);

  return <div className="chart" ref={node} role="img" aria-label={label} />;
}

export function AllocationChart({ snapshot }: { snapshot: Snapshot }) {
  const { locale, t } = useLocale();
  const ranked = snapshot.holdings.filter((holding) => (holding.weight ?? 0) > 0);
  const lead = ranked.slice(0, 5);
  const other = ranked.slice(5).reduce((total, holding) => total + (holding.weight ?? 0), 0);
  const data = lead.map((holding) => ({ name: holding.ticker, value: holding.weight ?? 0 }));
  if (other > 0) data.push({ name: t.otherHoldings, value: other });
  if (!data.length) return <div className="chart-empty">{t.noHoldingWeights}</div>;

  return (
    <div className="allocation">
      <div className="donut">
        <Chart
          label={`${t.holdingWeights}. ${data.map((item) => `${item.name} ${percent(item.value)}`).join(locale === "zh-CN" ? "，" : ", ")}`}
          option={{
            color: colors,
            tooltip: { ...tooltip, trigger: "item", valueFormatter: (value) => percent(Number(value)) },
            series: [{
              type: "pie", radius: ["65%", "91%"], center: ["50%", "50%"],
              label: { show: false }, labelLine: { show: false },
              itemStyle: { borderColor: "#fff", borderWidth: 3, borderRadius: 5 },
              data,
            }],
          }}
        />
        <div className="donut-center">
          <strong>{number(snapshot.holdings.length)}</strong>
          <span>{t.allocationCount(snapshot.holdings.length)}</span>
        </div>
      </div>
      <div className="chart-legend">
        {data.map((item, index) => (
          <div key={item.name}>
            <i style={{ background: colors[index] }} />
            <span>{item.name}</span><strong>{percent(item.value)}</strong>
          </div>
        ))}
      </div>
    </div>
  );
}

export function ValueChart({ points }: { points: HistoryPoint[] }) {
  const { locale, t } = useLocale();
  if (!points.some((point) => (point.valueUsd ?? 0) > 0)) {
    return <div className="chart-empty">{t.noValueHistory}</div>;
  }
  const limit = Math.max(...points.map((point) => point.valueUsd ?? 0));
  const minimum = Math.min(...points.flatMap((point) => point.valueUsd === null ? [] : [point.valueUsd]));
  const bounds = minimum === limit ? {} : { min: Math.max(0, minimum - (limit - minimum) * 0.25) };
  return (
    <div className="value-chart">
      <Chart
        label={`${t.valueTrend}. ${points.map((point) => `${quarterLabel(point.period, locale)} ${compact(point.valueUsd, locale)}`).join(locale === "zh-CN" ? "，" : ", ")}`}
        option={{
          grid: { top: 16, left: 6, right: 17, bottom: 11, outerBoundsMode: "same", outerBounds: { left: 0, right: 0, top: 0, bottom: 0 } },
          tooltip: { ...tooltip, trigger: "axis", valueFormatter: (value) => compact(Number(value), locale) },
          xAxis: {
            type: "category", boundaryGap: true,
            data: points.map((point) => quarterLabel(point.period, locale)),
            axisLabel: { color: "#718099", fontSize: 10, hideOverlap: true, margin: 12, formatter: (label: string, index: number) => points[index] ? quarterLabel(points[index].period, locale, true) : label },
            axisTick: { show: false }, axisLine: { lineStyle: { color: "#e8ebf3" } },
          },
          yAxis: {
            type: "value", ...bounds, splitNumber: 4,
            axisLabel: { color: "#718099", fontSize: 10, formatter: (value: number) => compact(value, locale, 1) },
            splitLine: { lineStyle: { color: "#ebedf5", type: "dashed" } },
          },
          series: [{
            name: t.portfolioValue, type: "line", smooth: 0.33,
            data: points.map((point) => point.valueUsd),
            symbol: "circle", symbolSize: 7,
            lineStyle: { width: 3, color: "#685cf2" },
            itemStyle: { color: "#685cf2", borderColor: "#fff", borderWidth: 1.5 },
            areaStyle: {
              color: {
                type: "linear", x: 0, y: 0, x2: 0, y2: 1,
                colorStops: [{ offset: 0, color: "#8374f44d" }, { offset: 1, color: "#aaa2ff05" }],
              },
            },
          }],
        }}
      />
    </div>
  );
}

export function SectorChart({ sectors }: { sectors: Sector[] }) {
  const { locale, t } = useLocale();
  if (!sectors.length) return <div className="chart-empty">{t.noSectorWeights}</div>;
  const ordered = [...sectors].sort((a, b) => a.weight - b.weight).map((sector) => ({
    ...sector, name: sectorName(sector.name, locale, t.unclassified),
  }));
  return (
    <div className="sector-chart" style={{ height: Math.max(175, ordered.length * 30 + 38) }}>
      <Chart
        label={`${t.sectorAllocation}. ${ordered.map((sector) => `${sector.name} ${percent(sector.weight)}`).join(locale === "zh-CN" ? "，" : ", ")}`}
        option={{
          grid: { top: 6, bottom: 8, left: 4, right: 47, outerBoundsMode: "same", outerBounds: { left: 0, right: 0, top: 0, bottom: 0 } },
          tooltip: { ...tooltip, trigger: "axis", valueFormatter: (value) => percent(Number(value)) },
          xAxis: {
            type: "value", axisLabel: { color: "#718099", formatter: "{value}%" },
            splitLine: { lineStyle: { color: "#ebedf5", type: "dashed" } },
          },
          yAxis: {
            type: "category", data: ordered.map((sector) => sector.name),
            axisTick: { show: false }, axisLine: { show: false },
            axisLabel: { color: "#526077", fontSize: 11, width: 130, overflow: "truncate" },
          },
          series: [{
            name: t.chartPortfolioWeight, type: "bar", data: ordered.map((sector) => sector.weight), barMaxWidth: 13,
            itemStyle: { color: "#786af3", borderRadius: [0, 5, 5, 0] },
            label: { show: true, position: "right", color: "#50607a", formatter: ({ value }) => percent(Number(value)) },
          }],
        }}
      />
    </div>
  );
}
