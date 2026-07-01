export type SdkWorkApiResponse<TData> = {
  code: 0;
  data: TData;
  traceId: string;
};

export type SdkWorkProblemDetail = {
  type: string;
  title: string;
  status: number;
  code: number;
  traceId: string;
  detail?: string;
  errors?: Array<{ field: string; message: string; code?: number }>;
};

export const SDKWORK_SUCCESS_CODE = 0 as const;

/** @deprecated Use SdkWorkApiResponse */
export type ApiEnvelope<T> = SdkWorkApiResponse<T>;
