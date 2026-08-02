import type { jsPDF } from "jspdf";
import type { UserOptions } from "jspdf-autotable";

declare module "jspdf" {
  interface jsPDF {
    autoTable: (options: UserOptions) => void;
    lastAutoTable: { finalY: number } | false;
  }
}
