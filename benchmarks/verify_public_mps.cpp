// Isolated HiGHS reference reader; never part of the production Rust build.
#include "Highs.h"
#include <cmath>
#include <iomanip>
#include <iostream>

template <typename T>
void array(const char* name, const std::vector<T>& values, bool bounds = false) {
  std::cout << ",\"" << name << "\":[";
  for (size_t i = 0; i < values.size(); ++i) {
    if (i) std::cout << ',';
    if (bounds && std::abs(double(values[i])) >= kHighsInf) std::cout << "null";
    else std::cout << values[i];
  }
  std::cout << ']';
}

int main(int argc, char** argv) {
  if (argc != 2) return 2;
  Highs highs;
  highs.setOptionValue("output_flag", false);
  if (highs.readModel(argv[1]) != HighsStatus::kOk) return 3;
  HighsLp lp = highs.getLp();
  lp.ensureRowwise();
  std::cout << std::setprecision(17) << "{\"sense\":\""
            << (lp.sense_ == ObjSense::kMinimize ? "minimize" : "maximize")
            << "\",\"objective_offset\":" << lp.offset_;
  array("objective", lp.col_cost_);
  array("lower", lp.col_lower_, true);
  array("upper", lp.col_upper_, true);
  array("row_lower", lp.row_lower_, true);
  array("row_upper", lp.row_upper_, true);
  array("indptr", lp.a_matrix_.start_);
  array("indices", lp.a_matrix_.index_);
  array("values", lp.a_matrix_.value_);
  std::cout << ",\"kinds\":[";
  for (HighsInt j = 0; j < lp.num_col_; ++j) {
    if (j) std::cout << ',';
    auto kind = lp.integrality_.empty() ? HighsVarType::kContinuous : lp.integrality_[j];
    if (kind != HighsVarType::kContinuous && kind != HighsVarType::kInteger) return 4;
    std::cout << (kind == HighsVarType::kContinuous ? "\"continuous\"" : "\"integer\"");
  }
  std::cout << "]}\n";
}
