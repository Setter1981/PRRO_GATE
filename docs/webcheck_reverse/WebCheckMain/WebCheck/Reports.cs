using System;
using System.ComponentModel;
using System.Data.SQLite;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class Reports
{
	public TypErr Reprt1(string sh)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "with t as ( \r\n            SELECT TAXCODE as TX ,TAXPRC AS TXPR,NULL AS TX0,SUM(TAXSUM) AS TXI, NULL AS SMI, NULL AS SMO from CHECKTAX  where CHECKID in (select ID FROM CHECKHEAD WHERE SHIFTID ='" + sh + "' AND DOCTYPE = '0' ) GROUP BY TAXCODE \r\n            union all \r\n            SELECT TAXCODE as TX ,TAXPRC AS TXPR,SUM(TAXSUM) AS TX0,NULL AS TXI, NULL AS SMI, NULL AS SMO  from CHECKTAX  where CHECKID in (select ID FROM CHECKHEAD WHERE SHIFTID ='" + sh + "' AND DOCTYPE = '1' ) GROUP BY TAXCODE \r\n            union all \r\n            SELECT LETTER as TX,0 AS TXPR,NULL AS TX0,NULL AS TXI, SUM(COST) AS SMI, NULL AS SMO FROM CHECKBODY WHERE CHECKID in (select ID FROM CHECKHEAD WHERE SHIFTID ='" + sh + "' AND DOCTYPE = '0' ) GROUP BY LETTER \r\n            union all \r\n            SELECT LETTER as TX,0 AS TXPR,NULL AS TX0,NULL AS TXI, NULL AS SMI,SUM(COST) AS SMO FROM CHECKBODY WHERE CHECKID in (select ID FROM CHECKHEAD WHERE SHIFTID ='" + sh + "' AND DOCTYPE = '1' ) GROUP BY LETTER   \r\n            ) select TX,MAX(TXPR),sum(TX0),sum(TXI),sum(SMI),sum(SMO) from t GROUP by TX;";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			int num = 0;
			while (sQLiteDataReader.Read())
			{
				num = checked(num + 1);
				All.PayTax.set_ReportsSum(0, num, sQLiteDataReader[0].ToString());
				All.PayTax.set_ReportsSum(1, num, sQLiteDataReader[1].ToString());
				All.PayTax.set_ReportsSum(2, num, sQLiteDataReader[2].ToString());
				All.PayTax.set_ReportsSum(3, num, sQLiteDataReader[3].ToString());
				All.PayTax.set_ReportsSum(4, num, sQLiteDataReader[4].ToString());
				All.PayTax.set_ReportsSum(5, num, sQLiteDataReader[5].ToString());
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 18;
			result.errStr = "Ошибка при формировании ZX отчета";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypErr Reprt2(string sh)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "with t as (\r\n            SELECT PAYMENTFORM as NM ,SUM(TOTALSUM) AS SMI, NULL AS SMO from CHECKPAY  where CHECKID in (select ID FROM CHECKHEAD WHERE SHIFTID ='" + sh + "' AND DOCTYPE = '0' ) GROUP BY PAYMENTFORM\r\n            union all\r\n            SELECT PAYMENTFORM as NM ,NULL AS SMI, SUM(TOTALSUM) AS SMO   from CHECKPAY  where CHECKID in (select ID FROM CHECKHEAD WHERE SHIFTID ='" + sh + "' AND DOCTYPE = '1' ) GROUP BY PAYMENTFORM\r\n            ) select NM,sum(SMI),sum(SMO) from t GROUP by NM;";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			int num = 0;
			while (sQLiteDataReader.Read())
			{
				num = checked(num + 1);
				All.PayTax.set_ReportsPay(0, num, sQLiteDataReader[0].ToString());
				All.PayTax.set_ReportsPay(1, num, sQLiteDataReader[1].ToString());
				All.PayTax.set_ReportsPay(2, num, sQLiteDataReader[2].ToString());
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 18;
			result.errStr = "Ошибка при формировании ZX отчета";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypErr Reprt3(string sh)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "with t as (\r\n            SELECT 'checkcount' as checkcount ,count(id) as NI ,NULL AS NO from CHECKHEAD where  SHIFTID ='" + sh + "' AND DOCTYPE = '0' \r\n            union all\r\n            SELECT 'checkcount' as checkcount ,NULL as NI ,count(id) AS NO  from CHECKHEAD  where  SHIFTID ='" + sh + "' AND DOCTYPE = '1' \r\n            ) select checkcount,sum(NI),sum(NO) from t GROUP by checkcount;";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				All.PayTax.Ni = sQLiteDataReader[1].ToString();
				All.PayTax.No = sQLiteDataReader[2].ToString();
			}
			else
			{
				All.PayTax.Ni = "0.00";
				All.PayTax.No = "0.00";
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 18;
			result.errStr = "Ошибка при формировании ZX отчета";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypErr Reprt4(string sh)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "with t as (\r\n            SELECT PAYMENTFORM as NM ,SUM(TOTALSUM) AS SMI, NULL AS SMO from CHECKPAY  where CHECKID in (select ID FROM CHECKHEAD WHERE SHIFTID ='" + sh + "' AND DOCTYPE = '3' ) and PAYMENTFORM = 'Готівка'  GROUP BY PAYMENTFORM\r\n            union all\r\n            SELECT PAYMENTFORM as NM ,NULL AS SMI, SUM(TOTALSUM) AS SMO   from CHECKPAY  where CHECKID in (select ID FROM CHECKHEAD WHERE SHIFTID ='" + sh + "' AND DOCTYPE = '4' ) and PAYMENTFORM = 'Готівка'   GROUP BY PAYMENTFORM\r\n            ) select NM,sum(SMI),sum(SMO) from t GROUP by NM;";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				All.PayTax.SMI = sQLiteDataReader[1].ToString();
				All.PayTax.SMO = sQLiteDataReader[2].ToString();
			}
			else
			{
				All.PayTax.SMI = "0.00";
				All.PayTax.SMO = "0.00";
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 18;
			result.errStr = "Ошибка при формировании ZX отчета";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypErrStr Reprt5(string cns, int tnk = 1, string FNs = "")
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			if (All.A.Status)
			{
				sQLiteConnection.ConnectionString = All.A.Connection;
			}
			else
			{
				string text = All.f.StringGetFn(FNs, "Path");
				sQLiteConnection.ConnectionString = "Data Source=" + text + "; Version=3";
			}
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM ksef WHERE checkidficscal='" + cns + "' order by id DESC";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			cns = cns.Replace("_", "`");
			if (sQLiteDataReader.Read())
			{
				if (tnk < 0)
				{
					tnk = 0;
				}
				if (tnk > 12)
				{
					tnk = 12;
				}
				result.ReturnStr = sQLiteDataReader[tnk].ToString();
			}
			else
			{
				result.errCode = 22;
				result.errStr = "У контрольній стрічці немає чека з номером: " + cns;
				result.ReturnStr = "";
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 21;
			result.errStr = "Помилка при отриманні XML чека з контрольної стрічки.";
			result.ReturnStr = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypErrOborot Report6(string sh)
	{
		TypErrOborot result = default(TypErrOborot);
		result.errCode = 0;
		result.errStr = "";
		result.SumTXA = "";
		result.SumTXGA = "";
		result.SumTXDA = "";
		result.SumTXAret = "";
		result.SumTXGAret = "";
		result.SumTXDAret = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "with t as\r\n(\r\nSELECT 1 as GROUPT ,SUM(COST) AS TXA,NULL AS TXGA, NULL AS TXDA, NULL AS TXARET, NULL AS TXGARET, NULL AS TXDARET from CHECKBODY  where (CHECKID in (select ID FROM CHECKHEAD WHERE SHIFTID = '" + sh + "' AND DOCTYPE = '0' )AND LETTER = 'А') GROUP BY LETTER\r\nunion all\r\nSELECT 1 as GROUPT ,NULL AS TXA,NULL AS TXGA, NULL AS TXDA, SUM(COST)  AS TXARET, NULL AS TXGARET, NULL AS TXDARET from CHECKBODY  where (CHECKID in (select ID FROM CHECKHEAD WHERE SHIFTID = '" + sh + "' AND DOCTYPE = '1' )AND LETTER = 'А') GROUP BY LETTER\r\nunion all\r\nSELECT 1 as GROUPT ,NULL AS TXA,SUM(COST) AS TXGA, NULL AS TXDA, NULL AS TXARET, NULL AS TXGARET, NULL AS TXDARET from CHECKBODY  where (CHECKID in (select ID FROM CHECKHEAD WHERE SHIFTID = '" + sh + "' AND DOCTYPE = '0' )AND LETTER = 'ГА') GROUP BY LETTER\r\nunion all\r\nSELECT 1 as GROUPT ,NULL AS TXA,NULL AS TXGA, NULL AS TXDA, NULL  AS TXARET, SUM(COST)  TXGARET, NULL AS TXDARET from CHECKBODY  where (CHECKID in (select ID FROM CHECKHEAD WHERE SHIFTID = '" + sh + "' AND DOCTYPE = '1' )AND LETTER = 'ГА') GROUP BY LETTER\r\nunion all\r\nSELECT 1 as GROUPT ,NULL AS TXA,NULL AS TXGA, SUM(COST) AS TXDA, NULL AS TXARET, NULL AS TXGARET, NULL AS TXDARET from CHECKBODY  where (CHECKID in (select ID FROM CHECKHEAD WHERE SHIFTID = '" + sh + "' AND DOCTYPE = '0' )AND LETTER = 'ДА') GROUP BY LETTER\r\nunion all\r\nSELECT 1 as GROUPT ,NULL AS TXA,NULL AS TXGA, NULL AS TXDA, NULL  AS TXARET, NULL TXGARET, SUM(COST) AS TXDARET from CHECKBODY  where (CHECKID in (select ID FROM CHECKHEAD WHERE SHIFTID = '" + sh + "' AND DOCTYPE = '1' )AND LETTER = 'ДА') GROUP BY LETTER\r\n)\r\nselect GROUPT,SUM(TXA),SUM(TXGA),SUM(TXDA),SUM(TXARET),SUM(TXGARET),SUM(TXDARET)\r\n  from t GROUP BY GROUPT";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			while (sQLiteDataReader.Read())
			{
				result.SumTXA = sQLiteDataReader[1].ToString();
				result.SumTXGA = sQLiteDataReader[2].ToString();
				result.SumTXDA = sQLiteDataReader[3].ToString();
				result.SumTXAret = sQLiteDataReader[4].ToString();
				result.SumTXGAret = sQLiteDataReader[5].ToString();
				result.SumTXDAret = sQLiteDataReader[6].ToString();
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 18;
			result.errStr = "Ошибка получения оборотов по налогам";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypErrEP Report7(string sh)
	{
		TypErrEP result = default(TypErrEP);
		result.errCode = 0;
		result.errStr = "";
		result.EPC = "";
		result.EPSM = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT count(id) as  EPC , sum(TOTALSUM) AS EPSM from CHECKHEAD where SHIFTID = '" + sh + "' AND DOCTYPE = '-8'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			while (sQLiteDataReader.Read())
			{
				result.EPC = sQLiteDataReader[0].ToString();
				result.EPSM = sQLiteDataReader[1].ToString();
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 18;
			result.errStr = "Ошибка получения информации о выдачи нала";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypSMB ReportSMB(string sh)
	{
		TypSMB result = default(TypSMB);
		result.SMIM = "";
		result.SMIP = "";
		result.SMOM = "";
		result.SMOP = "";
		result.SMB = false;
		int num = 1;
		do
		{
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = All.A.Connection;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				switch (num)
				{
				case 1:
					sQLiteCommand.CommandText = "SELECT SUM (CASHDESKNUM)  FROM CHECKHEAD WHERE (DocType =  0 and CASHDESKNUM<0 and shiftid ='" + sh + "')";
					break;
				case 2:
					sQLiteCommand.CommandText = "SELECT SUM (CASHDESKNUM)  FROM CHECKHEAD WHERE (DocType =  0 and CASHDESKNUM>0  and shiftid ='" + sh + "')";
					break;
				case 3:
					sQLiteCommand.CommandText = "SELECT SUM (CASHDESKNUM)  FROM CHECKHEAD WHERE (DocType =  1 and CASHDESKNUM<0  and shiftid ='" + sh + "')";
					break;
				case 4:
					sQLiteCommand.CommandText = "SELECT SUM (CASHDESKNUM)  FROM CHECKHEAD WHERE (DocType =  1 and CASHDESKNUM>0  and shiftid ='" + sh + "')";
					break;
				}
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				while (sQLiteDataReader.Read())
				{
					switch (num)
					{
					case 1:
						result.SMIM = sQLiteDataReader[0].ToString();
						if (Operators.CompareString(result.SMIM.Trim(), "", false) == 0)
						{
							result.SMIM = "0";
						}
						else
						{
							result.SMB = true;
						}
						break;
					case 2:
						result.SMIP = sQLiteDataReader[0].ToString();
						if (Operators.CompareString(result.SMIP.Trim(), "", false) == 0)
						{
							result.SMIP = "0";
						}
						else
						{
							result.SMB = true;
						}
						break;
					case 3:
						result.SMOM = sQLiteDataReader[0].ToString();
						if (Operators.CompareString(result.SMOM.Trim(), "", false) == 0)
						{
							result.SMOM = "0";
						}
						else
						{
							result.SMB = true;
						}
						break;
					case 4:
						result.SMOP = sQLiteDataReader[0].ToString();
						if (Operators.CompareString(result.SMOP.Trim(), "", false) == 0)
						{
							result.SMOP = "0";
						}
						else
						{
							result.SMB = true;
						}
						break;
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
				switch (num)
				{
				case 1:
					result.SMIM = "0";
					break;
				case 2:
					result.SMIP = "0";
					break;
				case 3:
					result.SMOM = "0";
					break;
				case 4:
					result.SMOP = "0";
					break;
				}
				ProjectData.ClearProjectError();
			}
			num = checked(num + 1);
		}
		while (num <= 4);
		result.SMIM = sABS(result.SMIM);
		result.SMIP = sABS(result.SMIP);
		result.SMOM = sABS(result.SMOM);
		result.SMOP = sABS(result.SMOP);
		return result;
	}

	private string sABS(string s)
	{
		double value;
		try
		{
			value = All.StrToDouble(s);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			value = 0.0;
			ProjectData.ClearProjectError();
		}
		return Math.Abs(value).ToString();
	}

	public string EPZtoCash(string sh)
	{
		string result;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT SUM(TOTALSUM) AS SMO from CHECKPAY where CHECKID in (select ID FROM CHECKHEAD WHERE SHIFTID ='" + sh + "' AND DOCTYPE = '-8' )  GROUP BY PAYMENTFORM;";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			result = ((!sQLiteDataReader.Read()) ? "0.00" : sQLiteDataReader[0].ToString());
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = "0.00";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public int NumberOfChecks(int nShift)
	{
		int result;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM SHIFTS WHERE ID=" + nShift;
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			string text = "0";
			text = ((!sQLiteDataReader.Read()) ? "0" : sQLiteDataReader[12].ToString());
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
			result = Conversions.ToInteger(text);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = 0;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypPrintChecks CheckXMLNumberTax(string TaxNumberCheck)
	{
		TypPrintChecks typPrintChecks = default(TypPrintChecks);
		typPrintChecks.ReturnStr = "";
		typPrintChecks.ReturnStrN = "";
		typPrintChecks.ReturnStrTaxN = "";
		typPrintChecks.ReturnID = "";
		typPrintChecks.ReturnMac = "";
		typPrintChecks.ReturnOffline = "";
		typPrintChecks.ReturnSum = "";
		typPrintChecks.ReturnDocType = "";
		TypPrintChecks result;
		if (Operators.CompareString(TaxNumberCheck.Trim(), "", false) == 0)
		{
			result = typPrintChecks;
		}
		else
		{
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				new SQLiteCommand();
				sQLiteConnection.ConnectionString = All.A.Connection;
				sQLiteConnection.Open();
				SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "SELECT * FROM ksef WHERE checkidficscal='" + TaxNumberCheck + "' order by id DESC";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				string text = "";
				string text2 = "";
				string text3 = "";
				string text4 = "";
				string text5 = "";
				string text6 = "";
				string text7 = "";
				string text8 = "";
				if (sQLiteDataReader.Read())
				{
					text2 = sQLiteDataReader[0].ToString();
					text = sQLiteDataReader[1].ToString();
					text3 = sQLiteDataReader[4].ToString();
					text4 = sQLiteDataReader[12].ToString();
					text5 = sQLiteDataReader[7].ToString();
					text6 = sQLiteDataReader[8].ToString();
					text7 = sQLiteDataReader[11].ToString();
					text8 = sQLiteDataReader[6].ToString();
				}
				else
				{
					text = "";
					text2 = "";
					text3 = "";
					text4 = "";
					text5 = "";
					text6 = "";
					text7 = "";
					text8 = "";
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
				typPrintChecks.ReturnOffline = text4;
				typPrintChecks.ReturnSum = text5;
				typPrintChecks.ReturnMac = text6;
				typPrintChecks.ReturnID = text7;
				typPrintChecks.ReturnStr = All.d.TegXml(text);
				typPrintChecks.ReturnStrN = text2;
				typPrintChecks.ReturnStrTaxN = text3;
				typPrintChecks.ReturnDocType = text8;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				typPrintChecks.ReturnStr = "";
				typPrintChecks.ReturnStrN = "";
				typPrintChecks.ReturnStrTaxN = "";
				typPrintChecks.ReturnID = "";
				typPrintChecks.ReturnMac = "";
				typPrintChecks.ReturnOffline = "";
				typPrintChecks.ReturnSum = "";
				typPrintChecks.ReturnDocType = "";
				result = typPrintChecks;
				ProjectData.ClearProjectError();
				goto IL_027e;
			}
			result = typPrintChecks;
		}
		goto IL_027e;
		IL_027e:
		return result;
	}

	public string CheckSumNumberTax(string TaxNumberCheck)
	{
		string text = "";
		string result;
		if (Operators.CompareString(TaxNumberCheck.Trim(), "", false) == 0)
		{
			result = text;
		}
		else
		{
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				new SQLiteCommand();
				sQLiteConnection.ConnectionString = All.A.Connection;
				sQLiteConnection.Open();
				SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "SELECT (ksef.sum-COALESCE(CHECKHEAD.CASHDESKNUM, 0)) as SumO FROM ksef left join CHECKHEAD ON ksef.checkid = CHECKHEAD.ID where checkidficscal='" + TaxNumberCheck + "'";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				if (sQLiteDataReader.Read())
				{
					text = sQLiteDataReader[0].ToString();
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = text;
				ProjectData.ClearProjectError();
				goto IL_009b;
			}
			result = text;
		}
		goto IL_009b;
		IL_009b:
		return result;
	}

	public TypPrintChecks CheckXMLNumber(string NumberCheck, bool SearchID = false)
	{
		TypPrintChecks typPrintChecks = default(TypPrintChecks);
		typPrintChecks.ReturnStr = "";
		typPrintChecks.ReturnStrN = "";
		typPrintChecks.ReturnStrTaxN = "";
		typPrintChecks.ReturnID = "";
		typPrintChecks.ReturnMac = "";
		typPrintChecks.ReturnOffline = "";
		typPrintChecks.ReturnSum = "";
		typPrintChecks.ReturnDocType = "";
		TypPrintChecks result;
		if (Operators.CompareString(NumberCheck.Trim(), "", false) == 0)
		{
			result = CheckXMLNumber();
		}
		else
		{
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = All.A.Connection;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				if (!SearchID)
				{
					sQLiteCommand.CommandText = "SELECT * FROM ksef WHERE checkid='" + NumberCheck + "'";
				}
				else
				{
					sQLiteCommand.CommandText = "SELECT * FROM ksef WHERE ID='" + NumberCheck + "'";
				}
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				string text = "";
				string text2 = "";
				string text3 = "";
				string text4 = "";
				string text5 = "";
				string text6 = "";
				string text7 = "";
				if (sQLiteDataReader.Read())
				{
					text2 = sQLiteDataReader[0].ToString();
					text = sQLiteDataReader[1].ToString();
					text3 = sQLiteDataReader[4].ToString();
					text4 = sQLiteDataReader[12].ToString();
					text5 = sQLiteDataReader[7].ToString();
					text6 = sQLiteDataReader[8].ToString();
					text7 = sQLiteDataReader[11].ToString();
				}
				else
				{
					text = "";
					text2 = "";
					text3 = "";
					text4 = "";
					text5 = "";
					text6 = "";
					text7 = "";
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
				typPrintChecks.ReturnOffline = text4;
				typPrintChecks.ReturnSum = text5;
				typPrintChecks.ReturnMac = text6;
				typPrintChecks.ReturnID = text7;
				typPrintChecks.ReturnStr = All.d.TegXml(text);
				typPrintChecks.ReturnStrN = text2;
				typPrintChecks.ReturnStrTaxN = text3;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				typPrintChecks.ReturnStr = "";
				typPrintChecks.ReturnStrN = "";
				typPrintChecks.ReturnStrTaxN = "";
				typPrintChecks.ReturnID = "";
				typPrintChecks.ReturnMac = "";
				typPrintChecks.ReturnOffline = "";
				typPrintChecks.ReturnSum = "";
				typPrintChecks.ReturnDocType = "";
				result = typPrintChecks;
				ProjectData.ClearProjectError();
				goto IL_027f;
			}
			result = typPrintChecks;
		}
		goto IL_027f;
		IL_027f:
		return result;
	}

	public TypPrintChecks CheckXMLNumber()
	{
		TypPrintChecks typPrintChecks = default(TypPrintChecks);
		typPrintChecks.ReturnStr = "";
		typPrintChecks.ReturnStrN = "";
		typPrintChecks.ReturnStrTaxN = "";
		typPrintChecks.ReturnID = "";
		typPrintChecks.ReturnMac = "";
		typPrintChecks.ReturnOffline = "";
		typPrintChecks.ReturnSum = "";
		typPrintChecks.ReturnDocType = "";
		TypPrintChecks result;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM ksef WHERE checkid='" + All.l.MaxID("CHECKHEAD").ReturnStr + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			string text = "";
			string text2 = "";
			string text3 = "";
			string text4 = "";
			string text5 = "";
			string text6 = "";
			string text7 = "";
			if (sQLiteDataReader.Read())
			{
				text2 = sQLiteDataReader[0].ToString();
				text = sQLiteDataReader[1].ToString();
				text3 = sQLiteDataReader[4].ToString();
				text4 = sQLiteDataReader[12].ToString();
				text5 = sQLiteDataReader[7].ToString();
				text6 = sQLiteDataReader[8].ToString();
				text7 = sQLiteDataReader[11].ToString();
			}
			else
			{
				text = "";
				text2 = "";
				text3 = "";
				text4 = "";
				text5 = "";
				text6 = "";
				text7 = "";
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
			typPrintChecks.ReturnOffline = text4;
			typPrintChecks.ReturnSum = text5;
			typPrintChecks.ReturnMac = text6;
			typPrintChecks.ReturnID = text7;
			typPrintChecks.ReturnStr = All.d.TegXml(text);
			typPrintChecks.ReturnStrN = text2;
			typPrintChecks.ReturnStrTaxN = text3;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			typPrintChecks.ReturnStr = "";
			typPrintChecks.ReturnStrN = "";
			typPrintChecks.ReturnStrTaxN = "";
			typPrintChecks.ReturnID = "";
			typPrintChecks.ReturnMac = "";
			typPrintChecks.ReturnOffline = "";
			typPrintChecks.ReturnSum = "";
			typPrintChecks.ReturnDocType = "";
			result = typPrintChecks;
			ProjectData.ClearProjectError();
			goto IL_0251;
		}
		result = typPrintChecks;
		goto IL_0251;
		IL_0251:
		return result;
	}

	public TypPrintChecks CheckXMLlast()
	{
		TypPrintChecks typPrintChecks = default(TypPrintChecks);
		typPrintChecks.ReturnStr = "";
		typPrintChecks.ReturnStrN = "";
		typPrintChecks.ReturnStrTaxN = "";
		typPrintChecks.ReturnID = "";
		typPrintChecks.ReturnMac = "";
		typPrintChecks.ReturnOffline = "";
		typPrintChecks.ReturnSum = "";
		typPrintChecks.ReturnDocType = "";
		TypPrintChecks result;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM ksef WHERE (offline='0' or offline='1') ORDER BY id DESC LIMIT 1";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			string text = "";
			string text2 = "";
			string text3 = "";
			string text4 = "";
			string text5 = "";
			string text6 = "";
			string text7 = "";
			if (sQLiteDataReader.Read())
			{
				text2 = sQLiteDataReader[0].ToString();
				text = sQLiteDataReader[2].ToString();
				text3 = sQLiteDataReader[4].ToString();
				text4 = sQLiteDataReader[12].ToString();
				text5 = sQLiteDataReader[7].ToString();
				text6 = sQLiteDataReader[8].ToString();
				text7 = sQLiteDataReader[11].ToString();
			}
			else
			{
				text = "";
				text2 = "";
				text3 = "";
				text4 = "";
				text5 = "";
				text6 = "";
				text7 = "";
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
			typPrintChecks.ReturnOffline = text4;
			typPrintChecks.ReturnSum = text5;
			typPrintChecks.ReturnMac = text6;
			typPrintChecks.ReturnID = text7;
			typPrintChecks.ReturnStr = text;
			typPrintChecks.ReturnStrN = text2;
			typPrintChecks.ReturnStrTaxN = text3;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			typPrintChecks.ReturnStr = "";
			typPrintChecks.ReturnStrN = "";
			typPrintChecks.ReturnStrTaxN = "";
			typPrintChecks.ReturnID = "";
			typPrintChecks.ReturnMac = "";
			typPrintChecks.ReturnOffline = "";
			typPrintChecks.ReturnSum = "";
			typPrintChecks.ReturnDocType = "";
			result = typPrintChecks;
			ProjectData.ClearProjectError();
			goto IL_0229;
		}
		result = typPrintChecks;
		goto IL_0229;
		IL_0229:
		return result;
	}

	public TypPrintChecks CheckXMLz()
	{
		TypPrintChecks typPrintChecks = default(TypPrintChecks);
		typPrintChecks.ReturnStr = "";
		typPrintChecks.ReturnStrN = "";
		typPrintChecks.ReturnStrTaxN = "";
		typPrintChecks.ReturnID = "";
		typPrintChecks.ReturnMac = "";
		typPrintChecks.ReturnOffline = "";
		typPrintChecks.ReturnSum = "";
		typPrintChecks.ReturnDocType = "";
		TypPrintChecks result;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			int num = 0;
			TypErrStr typErrStr = All.l.ReturnOpenShift();
			if (typErrStr.errCode <= 0)
			{
				num = Conversions.ToInteger(typErrStr.ReturnStr);
				num = ((num <= 1) ? Conversions.ToInteger(All.l.MaxID("SHIFTS").ReturnStr) : checked(num - 1));
				SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "SELECT * FROM ksef WHERE checkid='" + num + "Z'";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				string text = "";
				string text2 = "";
				string text3 = "";
				string text4 = "";
				string text5 = "";
				string text6 = "";
				string text7 = "";
				if (sQLiteDataReader.Read())
				{
					text2 = sQLiteDataReader[0].ToString();
					text = sQLiteDataReader[1].ToString();
					text3 = sQLiteDataReader[4].ToString();
					text4 = sQLiteDataReader[12].ToString();
					text5 = sQLiteDataReader[7].ToString();
					text6 = sQLiteDataReader[8].ToString();
					text7 = sQLiteDataReader[11].ToString();
				}
				else
				{
					text = "";
					text2 = "";
					text3 = "";
					text4 = "";
					text5 = "";
					text6 = "";
					text7 = "";
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
				typPrintChecks.ReturnOffline = text4;
				typPrintChecks.ReturnSum = text5;
				typPrintChecks.ReturnMac = text6;
				typPrintChecks.ReturnID = text7;
				typPrintChecks.ReturnStr = text.Trim().ToLower();
				typPrintChecks.ReturnStrN = text2;
				typPrintChecks.ReturnStrTaxN = text3;
				goto IL_02a2;
			}
			result = typPrintChecks;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			typPrintChecks.ReturnStr = "";
			typPrintChecks.ReturnStrN = "";
			typPrintChecks.ReturnStrTaxN = "";
			typPrintChecks.ReturnID = "";
			typPrintChecks.ReturnMac = "";
			typPrintChecks.ReturnOffline = "";
			typPrintChecks.ReturnSum = "";
			typPrintChecks.ReturnDocType = "";
			result = typPrintChecks;
			ProjectData.ClearProjectError();
		}
		goto IL_02a4;
		IL_02a2:
		result = typPrintChecks;
		goto IL_02a4;
		IL_02a4:
		return result;
	}

	public TypPrintChecks CheckXMLID(string NumberCheck)
	{
		TypPrintChecks typPrintChecks = default(TypPrintChecks);
		typPrintChecks.ReturnStr = "";
		typPrintChecks.ReturnStrN = "";
		typPrintChecks.ReturnStrTaxN = "";
		typPrintChecks.ReturnID = "";
		typPrintChecks.ReturnMac = "";
		typPrintChecks.ReturnOffline = "";
		typPrintChecks.ReturnSum = "";
		typPrintChecks.ReturnDocType = "";
		TypPrintChecks result;
		if (Operators.CompareString(NumberCheck.Trim(), "", false) == 0)
		{
			result = CheckXMLNumber();
		}
		else
		{
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				new SQLiteCommand();
				sQLiteConnection.ConnectionString = All.A.Connection;
				sQLiteConnection.Open();
				SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "SELECT * FROM ksef WHERE ID='" + NumberCheck + "'";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				string text = "";
				string text2 = "";
				string text3 = "";
				string text4 = "";
				string text5 = "";
				string text6 = "";
				string text7 = "";
				if (sQLiteDataReader.Read())
				{
					text2 = sQLiteDataReader[0].ToString();
					text = sQLiteDataReader[1].ToString();
					text3 = sQLiteDataReader[4].ToString();
					text4 = sQLiteDataReader[12].ToString();
					text5 = sQLiteDataReader[7].ToString();
					text6 = sQLiteDataReader[8].ToString();
					text7 = sQLiteDataReader[11].ToString();
				}
				else
				{
					text = "";
					text2 = "";
					text3 = "";
					text4 = "";
					text5 = "";
					text6 = "";
					text7 = "";
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
				typPrintChecks.ReturnOffline = text4;
				typPrintChecks.ReturnSum = text5;
				typPrintChecks.ReturnMac = text6;
				typPrintChecks.ReturnID = text7;
				typPrintChecks.ReturnStr = text.Trim().ToLower();
				typPrintChecks.ReturnStrN = text2;
				typPrintChecks.ReturnStrTaxN = text3;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				typPrintChecks.ReturnStr = "";
				typPrintChecks.ReturnStrN = "";
				typPrintChecks.ReturnStrTaxN = "";
				typPrintChecks.ReturnID = "";
				typPrintChecks.ReturnMac = "";
				typPrintChecks.ReturnOffline = "";
				typPrintChecks.ReturnSum = "";
				typPrintChecks.ReturnDocType = "";
				result = typPrintChecks;
				ProjectData.ClearProjectError();
				goto IL_025e;
			}
			result = typPrintChecks;
		}
		goto IL_025e;
		IL_025e:
		return result;
	}
}
