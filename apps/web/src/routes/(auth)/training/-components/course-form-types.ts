import type { ComponentProps } from "react";

import { withForm } from "@/components/form/hooks";
import { courseFormDefaultValues } from "@/routes/(auth)/training/-components/training-form-utils";

const CourseFormForTyping = withForm({
  defaultValues: courseFormDefaultValues,
  render: () => null,
});

export type CourseFormInstance = ComponentProps<
  typeof CourseFormForTyping
>["form"];
