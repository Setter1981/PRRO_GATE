using System;
using System.ComponentModel;
using System.Data.SQLite;
using System.Xml;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

public class DateReport
{
	private string xmlPeriod;

	private TypTaxReport[] T;

	private TypPayReport[] P;

	private int NcNi;

	private int NcNo;

	private int zCount;

	private string zStart;

	private string zFinish;

	private string zStartD;

	private string zFinishD;

	private string zStartT;

	private string zFinishT;

	private string dtS;

	private string dtE;

	public DateReport()
	{
		checked
		{
			T = new TypTaxReport[All.PayTax.TaxN + 1];
			P = new TypPayReport[All.PayTax.PayN + 1];
		}
	}

	internal TypErrStr PeriodR(string strXML)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "startdate");
		TypErrStr parametrToString2 = All.d.GetParametrToString(strXML, "enddate");
		if (parametrToString.ReturnStr.Length != 8)
		{
			result.errCode = 86;
			result.errStr = "Укажите правильно StartDate";
			return result;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr))
		{
			result.errCode = 86;
			result.errStr = "Укажите правильно StartDate";
			return result;
		}
		if (parametrToString2.ReturnStr.Length != 8)
		{
			result.errCode = 86;
			result.errStr = "Укажите правильно EnaDate";
			return result;
		}
		if (!Versioned.IsNumeric((object)parametrToString2.ReturnStr))
		{
			result.errCode = 86;
			result.errStr = "Укажите правильно EnaDate";
			return result;
		}
		Zero();
		dtS = parametrToString.ReturnStr;
		dtE = parametrToString2.ReturnStr;
		parametrToString.ReturnStr = DateCorect(parametrToString.ReturnStr, StartD: true);
		parametrToString2.ReturnStr = DateCorect(parametrToString2.ReturnStr, StartD: false);
		TypErrStr typErrStr = ReprtPeriod(parametrToString.ReturnStr, parametrToString2.ReturnStr);
		if (typErrStr.errCode > 0)
		{
			result.errCode = typErrStr.errCode;
			result.errStr = typErrStr.errStr;
			All.Lg.SaveTextToLog("PeriodR", typErrStr.errStr, "Код ошибки: " + typErrStr.errCode);
			return result;
		}
		if (zCount < 1)
		{
			result.errCode = 87;
			result.errStr = "За указанные период Z отчетов нет";
			return result;
		}
		Payment();
		if (!XmlPeriodReport())
		{
			result.errCode = 87;
			result.errStr = "Ошибка формирования периодического отчета";
			All.Lg.SaveTextToLog("PeriodR", "Отчет не может быть показан", "Ошибка при создании XML документа");
			return result;
		}
		result.ReturnStr = xmlPeriod;
		All.Lg.SaveTextToLog("Периодический отчет", xmlPeriod);
		new PrintExportCheck().ExportToFile(xmlPeriod, "pZvit");
		return result;
	}

	private TypErrStr ReprtPeriod(string d1, string d2)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		checked
		{
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = All.A.Connection;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "SELECT checkxml FROM ksef WHERE Datetime(dt)>=Datetime('" + d1 + "') AND Datetime(dt)<=Datetime('" + d2 + "') AND DocType='80'";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				while (sQLiteDataReader.Read())
				{
					string text = sQLiteDataReader[0].ToString();
					zCount++;
					if (!DerebanXmlZ(text))
					{
						result.errCode = 68;
						result.errStr = "Ошибка открытия одного из Z отчетов";
						result.ReturnStr = "";
						All.Lg.SaveTextToLog("ReprtPeriod", "Ошибка разбора Z отчета:", text);
						return result;
					}
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result.errCode = 68;
				result.errStr = "Ошибка формирования отчета за период: " + ex2.Message;
				result.ReturnStr = "";
				ProjectData.ClearProjectError();
			}
			return result;
		}
	}

	private bool DerebanXmlZ(string XmlZ)
	{
		XmlDocument xmlDocument = new XmlDocument();
		bool result;
		try
		{
			xmlDocument.LoadXml(XmlZ.ToLower());
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_0459;
		}
		if (Operators.CompareString(zStart.Trim(), "", false) == 0)
		{
			zStart = All.d.GetParametrToString(XmlZ, "no", "rq/dat/z").ReturnStr;
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			zStartD = LongToData(innerText);
			zStartT = LongToTime(innerText);
			zFinish = zStart;
			zFinishD = zStartD;
			zFinishT = zStartT;
		}
		else
		{
			zFinish = All.d.GetParametrToString(XmlZ, "no", "rq/dat/z").ReturnStr;
			string innerText2 = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			zFinishD = LongToData(innerText2);
			zFinishT = LongToTime(innerText2);
		}
		string returnStr = All.d.GetParametrToString(XmlZ, "ni", "rq/dat/z/nc").ReturnStr;
		string returnStr2 = All.d.GetParametrToString(XmlZ, "no", "rq/dat/z/nc").ReturnStr;
		checked
		{
			if (Versioned.IsNumeric((object)returnStr))
			{
				NcNi += Conversions.ToInteger(returnStr);
			}
			if (Versioned.IsNumeric((object)returnStr2))
			{
				NcNo += Conversions.ToInteger(returnStr2);
			}
			XmlNodeList elementsByTagName = xmlDocument.GetElementsByTagName("m");
			int num = elementsByTagName.Count - 1;
			XmlDocument xmlDocument2 = new XmlDocument();
			int num2 = num;
			for (int i = 0; i <= num2; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr3 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr3, "", false) != 0)
				{
					string text = returnStr3.ToUpper().Trim();
					int num3 = All.PayTax.get_PayABCtoNum(text);
					if (num3 < 0)
					{
						num3 = All.PayTax.AddPayTemp(text);
						ref TypPayReport[] p = ref P;
						p = (TypPayReport[])Utils.CopyArray((Array)p, (Array)new TypPayReport[All.PayTax.PayN + 1]);
						P[All.PayTax.PayN].Name = text;
						P[All.PayTax.PayN].SMI = 0.0;
						P[All.PayTax.PayN].SMO = 0.0;
					}
					double num4 = All.StrToDouble(All.d.GetParametrToString(outerXml, "smi", "m").ReturnStr);
					double num5 = All.StrToDouble(All.d.GetParametrToString(outerXml, "smo", "m").ReturnStr);
					P[num3].SMI += num4;
					P[num3].SMO += num5;
				}
			}
			elementsByTagName = xmlDocument.GetElementsByTagName("txs");
			int num6 = elementsByTagName.Count - 1;
			for (int i = 0; i <= num6; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr4 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr4, "", false) != 0)
				{
					string nameTaxe = returnStr4.ToUpper().Trim();
					int taxIndex = All.PayTax.Search(nameTaxe).TaxIndex;
					double num7 = All.StrToDouble(All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr);
					double num8 = All.StrToDouble(All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr);
					T[taxIndex].SMI += num7;
					T[taxIndex].SMO += num8;
				}
			}
			result = true;
			goto IL_0459;
		}
		IL_0459:
		return result;
	}

	private void Payment()
	{
		Directorys payTax = All.PayTax;
		int taxN = payTax.TaxN;
		for (int i = 1; i <= taxN; i = checked(i + 1))
		{
			if ((T[i].SMI > 0.0) | (T[i].SMO > 0.0))
			{
				if (T[i].Name.Trim().Length == 1)
				{
					T[i].TXI = All.TaxAmountBig(T[i].SMI, T[i].TXPR);
					T[i].TXO = All.TaxAmountBig(T[i].SMO, T[i].TXPR);
				}
				else if (Operators.CompareString(Conversions.ToString(T[i].Name[0]).ToLower(), "д", false) == 0)
				{
					double num = All.TaxAmountBig(T[i].SMI, 27.5);
					double amount = T[i].SMI - num;
					T[i].DTI = All.TaxAmountr(amount, 7.5);
					double num2 = All.TaxAmountBig(T[i].SMO, 27.5);
					double amount2 = T[i].SMO - num2;
					T[i].DTO = All.TaxAmountr(amount2, 7.5);
					All.TaxAmountr(amount, 20.0);
					double tXI = All.TaxAmountr(amount, 20.0);
					All.TaxAmountr(amount2, 20.0);
					double tXO = All.TaxAmountr(amount2, 20.0);
					T[i].TXI = tXI;
					T[i].TXO = tXO;
					string name = T[i].Name;
					name = Conversions.ToString(name[1]);
					int num3 = Conversions.ToInteger(payTax.ABCtoNUM(name));
					T[num3].TXI += T[i].TXI;
					T[num3].TXO += T[i].TXO;
					T[num3].Visible = true;
					T[i].Visible = true;
				}
				else
				{
					T[i].DTI = All.TaxAmountBig(T[i].SMI, T[i].DTPR);
					T[i].DTO = All.TaxAmountBig(T[i].SMO, T[i].DTPR);
					string name2 = T[i].Name;
					name2 = Conversions.ToString(name2[1]);
					int num4 = Conversions.ToInteger(payTax.ABCtoNUM(name2));
					double amount3 = T[i].SMI - T[i].DTI;
					double amount4 = T[i].SMO - T[i].DTO;
					T[i].TXI = All.TaxAmountBig(amount3, T[num4].TXPR);
					T[i].TXO = All.TaxAmountBig(amount4, T[num4].TXPR);
					T[num4].TXI += T[i].TXI;
					T[num4].TXO += T[i].TXO;
					T[num4].Visible = true;
					T[i].Visible = true;
				}
			}
		}
		payTax = null;
	}

	private bool XmlPeriodReport()
	{
		string text = "";
		long num = All.СurrentCompDate();
		text = "";
		text = text + "<DAT FN='" + All.A.FN + "' TN='" + All.A.TIN + "' DI='WebCheck' ZN='0' V='1'>";
		text = text + "<Z NO='" + DatZapros() + "' DS='" + zStartD + "' DE='" + zFinishD + "' ALL='" + zCount + "' NS='" + zStart + "' NE='" + zFinish + "'>";
		_ = All.PayTax;
		int payN = All.PayTax.PayN;
		checked
		{
			for (int i = 0; i <= payN; i++)
			{
				if ((P[i].SMI > 0.0) | (P[i].SMO > 0.0))
				{
					string text2 = i.ToString();
					if (Operators.CompareString(text2.Trim(), "1", false) == 0)
					{
						text2 = "0";
					}
					text = text + "<M T='" + text2 + "' NM='" + P[i].Name + "' SMI='" + P[i].SMI + "' SMO='" + P[i].SMO + "'/>";
				}
			}
			_ = null;
			Directorys payTax = All.PayTax;
			int taxN = payTax.TaxN;
			for (int i = 1; i <= taxN; i++)
			{
				if ((T[i].SMI > 0.0) | (T[i].SMO > 0.0) | (T[i].TXI > 0.0) | (T[i].TXO > 0.0) | T[i].Visible)
				{
					payTax.ABCtoNUM(T[i].Name);
					_ = DateTime.Now;
					text = ((T[i].Name.Trim().Length != 1) ? (text + "<TXS N='" + All.PayTax.get_TaxName(i) + "' TXPR='" + All.Bablo(All.PayTax.get_TaxPRC(i)) + "' TXI='" + All.Bablo(T[i].TXI) + "' TXO='" + All.Bablo(T[i].TXO) + "' SMI='" + All.Bablo(T[i].SMI) + "' SMO='" + All.Bablo(T[i].SMO) + "' DTPR='" + All.Bablo(T[i].DTPR) + "' DTI='" + All.Bablo(T[i].DTI) + "' DTO='" + All.Bablo(T[i].DTO) + "' TXTY='0' TXAL='0'/>") : (text + "<TXS N='" + All.PayTax.get_TaxName(i) + "' TXPR='" + All.Bablo(All.PayTax.get_TaxPRC(i)) + "' TXI='" + All.Bablo(T[i].TXI) + "' TXO='" + All.Bablo(T[i].TXO) + "' SMI='" + All.Bablo(T[i].SMI) + "' SMO='" + All.Bablo(T[i].SMO) + "' DTPR='0.00' DTI='0.00' DTO='0' TXTY='0' TXAL='0'/>"));
				}
			}
			payTax = null;
			text += "</Z>";
			text = text + "<TS>" + num + "</TS>";
			text += "</DAT>";
			text = "<RQ V='1'>" + text + "</RQ>";
			if (All.d.VerifyXML(text))
			{
				xmlPeriod = text;
				return true;
			}
			if (Operators.CompareString(text.Trim(), "", false) == 0)
			{
				text = "XML не может быть пустой строкой";
			}
			All.Lg.SaveTextToLog("PeriodReport", "Ошибка XML", text);
			xmlPeriod = "";
			return false;
		}
	}

	private string DatZapros()
	{
		return string.Concat("ВIД " + DateCorectString(dtS), " ДО ", DateCorectString(dtE));
	}

	private string LongToData(string LongDT, bool ForLink = false)
	{
		if (LongDT.Length != 14)
		{
			return "дата";
		}
		if (!ForLink)
		{
			return Conversions.ToString(LongDT[6]) + Conversions.ToString(LongDT[7]) + "." + Conversions.ToString(LongDT[4]) + Conversions.ToString(LongDT[5]) + "." + Conversions.ToString(LongDT[0]) + Conversions.ToString(LongDT[1]) + Conversions.ToString(LongDT[2]) + Conversions.ToString(LongDT[3]);
		}
		return Conversions.ToString(LongDT[0]) + Conversions.ToString(LongDT[1]) + Conversions.ToString(LongDT[2]) + Conversions.ToString(LongDT[3]) + Conversions.ToString(LongDT[4]) + Conversions.ToString(LongDT[5]) + Conversions.ToString(LongDT[6]) + Conversions.ToString(LongDT[7]);
	}

	private string DateCorect(string DateNotDot, bool StartD)
	{
		string text = Conversions.ToString(DateNotDot[4]) + Conversions.ToString(DateNotDot[5]) + Conversions.ToString(DateNotDot[6]) + Conversions.ToString(DateNotDot[7]);
		string oneS = Conversions.ToString(DateNotDot[2]) + Conversions.ToString(DateNotDot[3]);
		string oneS2 = Conversions.ToString(DateNotDot[0]) + Conversions.ToString(DateNotDot[1]);
		string text2 = text + "-" + All.TwoS(oneS) + "-" + All.TwoS(oneS2);
		if (StartD)
		{
			return text2 + "  00:00:00";
		}
		return text2 + "  23:59:59";
	}

	private string DateCorectString(string DateNotDot)
	{
		string text = Conversions.ToString(DateNotDot[4]) + Conversions.ToString(DateNotDot[5]) + Conversions.ToString(DateNotDot[6]) + Conversions.ToString(DateNotDot[7]);
		string oneS = Conversions.ToString(DateNotDot[2]) + Conversions.ToString(DateNotDot[3]);
		string oneS2 = Conversions.ToString(DateNotDot[0]) + Conversions.ToString(DateNotDot[1]);
		return All.TwoS(oneS2) + "." + All.TwoS(oneS) + "." + text;
	}

	private void Zero()
	{
		xmlPeriod = "";
		zCount = 0;
		NcNi = 0;
		NcNo = 0;
		zStart = "";
		zFinish = "";
		zStartD = "";
		zFinishD = "";
		zStartT = "";
		zFinishT = "";
		dtS = "";
		dtE = "";
		int taxN = All.PayTax.TaxN;
		checked
		{
			for (int i = 0; i <= taxN; i++)
			{
				ref TypTaxReport reference = ref T[i];
				reference.Name = All.PayTax.get_TaxName(i);
				reference.SMI = 0.0;
				reference.SMO = 0.0;
				reference.DTI = 0.0;
				reference.DTO = 0.0;
				reference.TXI = 0.0;
				reference.TXO = 0.0;
				reference.TXPR = All.StrToDouble(All.PayTax.get_TaxPRC(i));
				reference.DTPR = All.StrToDouble(All.PayTax.get_TaxEXCISE(i));
				reference.Visible = false;
			}
			int payN = All.PayTax.PayN;
			for (int i = 0; i <= payN; i++)
			{
				ref TypPayReport reference2 = ref P[i];
				reference2.Name = All.PayTax.get_PayName(i);
				reference2.SMI = 0.0;
				reference2.SMO = 0.0;
			}
		}
	}

	private string LongToTime(string LongDT)
	{
		if (LongDT.Length != 14)
		{
			return "время";
		}
		return Conversions.ToString(LongDT[8]) + Conversions.ToString(LongDT[9]) + "-" + Conversions.ToString(LongDT[10]) + Conversions.ToString(LongDT[11]) + "-" + Conversions.ToString(LongDT[12]) + Conversions.ToString(LongDT[13]);
	}
}
