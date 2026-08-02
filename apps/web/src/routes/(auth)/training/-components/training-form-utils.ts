import { createCourseInput } from "@lindaflor/shared/schemas/training";
import type { z } from "zod";

export type CourseFormValues = z.output<typeof createCourseInput>;

export const courseFormDefaultValues: CourseFormValues = {
  title: "",
  is_published: false,
  sections: [],
};

export type LectureType = "video" | "pdf" | "link";

export function isLectureType(value: unknown): value is LectureType {
  return value === "video" || value === "pdf" || value === "link";
}
