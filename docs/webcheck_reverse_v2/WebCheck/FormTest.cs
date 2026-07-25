using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.CompilerServices;
using System.Text;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using WebCheck.My;

namespace WebCheck;

[DesignerGenerated]
internal class FormTest : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("StartTest")]
	private Button _StartTest;

	[CompilerGenerated]
	[AccessedThroughProperty("setting")]
	private Button _setting;

	[CompilerGenerated]
	[AccessedThroughProperty("osp")]
	private Button _osp;

	[CompilerGenerated]
	[AccessedThroughProperty("CopyURLb")]
	private Button _CopyURLb;

	[CompilerGenerated]
	[AccessedThroughProperty("OpenURLb")]
	private Button _OpenURLb;

	[CompilerGenerated]
	[AccessedThroughProperty("SetGetB")]
	private Button _SetGetB;

	private string innOp;

	private IniHGB ospus;

	[field: AccessedThroughProperty("FileInfo")]
	internal virtual TextBox FileInfo
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button StartTest
	{
		[CompilerGenerated]
		get
		{
			return _StartTest;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = StartTest_Click;
			Button startTest = _StartTest;
			if (startTest != null)
			{
				startTest.Click -= value2;
			}
			_StartTest = value;
			startTest = _StartTest;
			if (startTest != null)
			{
				startTest.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("TestResult")]
	internal virtual TextBox TestResult
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button setting
	{
		[CompilerGenerated]
		get
		{
			return _setting;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = setting_Click;
			Button button = _setting;
			if (button != null)
			{
				button.Click -= value2;
			}
			_setting = value;
			button = _setting;
			if (button != null)
			{
				button.Click += value2;
			}
		}
	}

	internal virtual Button osp
	{
		[CompilerGenerated]
		get
		{
			return _osp;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = osp_Click;
			Button button = _osp;
			if (button != null)
			{
				button.Click -= value2;
			}
			_osp = value;
			button = _osp;
			if (button != null)
			{
				button.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("UrlT")]
	internal virtual TextBox UrlT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button CopyURLb
	{
		[CompilerGenerated]
		get
		{
			return _CopyURLb;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = CopyURLb_Click;
			Button copyURLb = _CopyURLb;
			if (copyURLb != null)
			{
				copyURLb.Click -= value2;
			}
			_CopyURLb = value;
			copyURLb = _CopyURLb;
			if (copyURLb != null)
			{
				copyURLb.Click += value2;
			}
		}
	}

	internal virtual Button OpenURLb
	{
		[CompilerGenerated]
		get
		{
			return _OpenURLb;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = OpenURLb_Click;
			Button openURLb = _OpenURLb;
			if (openURLb != null)
			{
				openURLb.Click -= value2;
			}
			_OpenURLb = value;
			openURLb = _OpenURLb;
			if (openURLb != null)
			{
				openURLb.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("NameL")]
	internal virtual Label NameL
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button SetGetB
	{
		[CompilerGenerated]
		get
		{
			return _SetGetB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = SetGetB_Click;
			Button setGetB = _SetGetB;
			if (setGetB != null)
			{
				setGetB.Click -= value2;
			}
			_SetGetB = value;
			setGetB = _SetGetB;
			if (setGetB != null)
			{
				setGetB.Click += value2;
			}
		}
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormTest));
		this.FileInfo = new System.Windows.Forms.TextBox();
		this.StartTest = new System.Windows.Forms.Button();
		this.TestResult = new System.Windows.Forms.TextBox();
		this.setting = new System.Windows.Forms.Button();
		this.osp = new System.Windows.Forms.Button();
		this.UrlT = new System.Windows.Forms.TextBox();
		this.CopyURLb = new System.Windows.Forms.Button();
		this.OpenURLb = new System.Windows.Forms.Button();
		this.NameL = new System.Windows.Forms.Label();
		this.SetGetB = new System.Windows.Forms.Button();
		base.SuspendLayout();
		this.FileInfo.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.FileInfo.Location = new System.Drawing.Point(12, 12);
		this.FileInfo.Multiline = true;
		this.FileInfo.Name = "FileInfo";
		this.FileInfo.ScrollBars = System.Windows.Forms.ScrollBars.Vertical;
		this.FileInfo.Size = new System.Drawing.Size(675, 134);
		this.FileInfo.TabIndex = 0;
		this.FileInfo.TabStop = false;
		this.StartTest.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.StartTest.Location = new System.Drawing.Point(12, 152);
		this.StartTest.Name = "StartTest";
		this.StartTest.Size = new System.Drawing.Size(620, 41);
		this.StartTest.TabIndex = 0;
		this.StartTest.Text = "Перевірити";
		this.StartTest.UseVisualStyleBackColor = true;
		this.TestResult.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TestResult.Location = new System.Drawing.Point(12, 199);
		this.TestResult.Multiline = true;
		this.TestResult.Name = "TestResult";
		this.TestResult.ScrollBars = System.Windows.Forms.ScrollBars.Vertical;
		this.TestResult.Size = new System.Drawing.Size(675, 211);
		this.TestResult.TabIndex = 2;
		this.TestResult.TabStop = false;
		this.setting.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.setting.Location = new System.Drawing.Point(12, 513);
		this.setting.Name = "setting";
		this.setting.Size = new System.Drawing.Size(330, 37);
		this.setting.TabIndex = 3;
		this.setting.Text = "settings.ini";
		this.setting.UseVisualStyleBackColor = true;
		this.osp.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.osp.Location = new System.Drawing.Point(357, 513);
		this.osp.Name = "osp";
		this.osp.Size = new System.Drawing.Size(330, 37);
		this.osp.TabIndex = 4;
		this.osp.Text = "ospus.ini";
		this.osp.UseVisualStyleBackColor = true;
		this.UrlT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.UrlT.Location = new System.Drawing.Point(12, 434);
		this.UrlT.Name = "UrlT";
		this.UrlT.ReadOnly = true;
		this.UrlT.Size = new System.Drawing.Size(675, 30);
		this.UrlT.TabIndex = 5;
		this.UrlT.TabStop = false;
		this.CopyURLb.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.CopyURLb.Location = new System.Drawing.Point(12, 470);
		this.CopyURLb.Name = "CopyURLb";
		this.CopyURLb.Size = new System.Drawing.Size(330, 37);
		this.CopyURLb.TabIndex = 6;
		this.CopyURLb.Text = "Скопіювати  URL";
		this.CopyURLb.UseVisualStyleBackColor = true;
		this.OpenURLb.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OpenURLb.Location = new System.Drawing.Point(357, 470);
		this.OpenURLb.Name = "OpenURLb";
		this.OpenURLb.Size = new System.Drawing.Size(330, 37);
		this.OpenURLb.TabIndex = 7;
		this.OpenURLb.Text = "Відкрити  URL";
		this.OpenURLb.UseVisualStyleBackColor = true;
		this.NameL.AutoSize = true;
		this.NameL.Font = new System.Drawing.Font("Microsoft Sans Serif", 9f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NameL.Location = new System.Drawing.Point(12, 413);
		this.NameL.Name = "NameL";
		this.NameL.Size = new System.Drawing.Size(142, 18);
		this.NameL.TabIndex = 8;
		this.NameL.Text = "Рішення проблеми ";
		this.SetGetB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.SetGetB.Location = new System.Drawing.Point(638, 152);
		this.SetGetB.Name = "SetGetB";
		this.SetGetB.Size = new System.Drawing.Size(52, 41);
		this.SetGetB.TabIndex = 9;
		this.SetGetB.Text = "...";
		this.SetGetB.UseVisualStyleBackColor = true;
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(702, 561);
		base.Controls.Add(this.SetGetB);
		base.Controls.Add(this.NameL);
		base.Controls.Add(this.OpenURLb);
		base.Controls.Add(this.CopyURLb);
		base.Controls.Add(this.UrlT);
		base.Controls.Add(this.osp);
		base.Controls.Add(this.setting);
		base.Controls.Add(this.TestResult);
		base.Controls.Add(this.StartTest);
		base.Controls.Add(this.FileInfo);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormTest";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Перевірка налаштувань ПРРО";
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	public FormTest(string innOperator)
	{
		base.Load += FormTest_Load;
		ospus = new IniHGB(All.MyDoc() + "\\WebCheck\\ospus.ini");
		InitializeComponent();
		innOp = innOperator;
	}

	private void FormTest_Load(object sender, EventArgs e)
	{
		FileInfo.Text = GetTextForOutput(All.A.FileN);
		NameL.Text = "Відповіді на типові запитання:";
		UrlT.Text = "https://www.webchek.com.ua/helpie_faq/";
	}

	private string GetTextForOutput(string filePath)
	{
		if (!MyProject.Computer.FileSystem.FileExists(filePath))
		{
			throw new Exception("File Not Found: " + filePath);
		}
		StringBuilder stringBuilder = new StringBuilder();
		FileInfo fileInfo = MyProject.Computer.FileSystem.GetFileInfo(filePath);
		stringBuilder.Append("File: " + fileInfo.FullName);
		stringBuilder.Append("\r\n");
		stringBuilder.Append("CreationTime: " + fileInfo.CreationTime.ToString());
		stringBuilder.Append("\r\n");
		stringBuilder.Append("Modified: " + fileInfo.LastWriteTime.ToString());
		stringBuilder.Append("\r\n");
		stringBuilder.Append("Size: " + fileInfo.Length + " bytes");
		stringBuilder.Append("\r\n");
		return stringBuilder.ToString();
	}

	private void StartTest_Click(object sender, EventArgs e)
	{
		StartTest.Enabled = false;
		string section = "\\SOFTWARE\\Institute of Informational Technologies\\Certificate Authority-1.3\\End User\\Libraries\\Sign\\KeyMedia";
		string @string = ospus.GetString(section, "ShowErrors", "0");
		int num = (Versioned.IsNumeric(@string) ? Conversions.ToInteger(@string) : 0);
		int retriesPrt = All.RetriesPrt;
		All.RetriesPrt = 3;
		All.SF.ErrorShow(ShowWindows: true);
		StringBuilder stringBuilder = new StringBuilder();
		stringBuilder.Append("WebCheck");
		stringBuilder.Append("\r\n");
		stringBuilder.Append("Version: " + All.VersionDll());
		stringBuilder.Append("\r\n");
		stringBuilder.Append("TimeStart:" + DateTime.Now.ToString());
		stringBuilder.Append("\r\n");
		All.A.OperatorINN = innOp;
		TypErrStr typErrStr = new CheckLastCheck().NumberLastCheckTest(innOp);
		All.A.OperatorINN = "";
		stringBuilder.Append("TimeEnd: " + DateTime.Now.ToString());
		stringBuilder.Append("\r\n");
		if (typErrStr.errCode > 0)
		{
			stringBuilder.Append("ErrCode: " + typErrStr.errCode);
			stringBuilder.Append("\r\n");
			stringBuilder.Append("ErrString: " + typErrStr.errStr.ToString());
			stringBuilder.Append("\r\n");
			HelpL(typErrStr.errCode);
		}
		else
		{
			stringBuilder.Append("ErrCode: 0");
			stringBuilder.Append("\r\n");
			stringBuilder.Append("ErrString: Not");
			stringBuilder.Append("\r\n");
			stringBuilder.Append("Status: Ок!");
			stringBuilder.Append("\r\n");
			stringBuilder.Append("Last check number on the tax website:" + typErrStr.ReturnStr);
			stringBuilder.Append("\r\n");
		}
		TestResult.Text = stringBuilder.ToString();
		All.RetriesPrt = retriesPrt;
		if (num > 0)
		{
			All.SF.ErrorShow(ShowWindows: true);
		}
		else
		{
			All.SF.ErrorShow(ShowWindows: false);
		}
		StartTest.Enabled = true;
	}

	private void HelpL(int errN)
	{
		NameL.Text = "Рішення помилки №" + errN + ":";
		int num = errN;
		if (num == -14)
		{
			UrlT.Text = "https://www.webchek.com.ua/helpie_faq/status-14-msg-ne-zareiestrovano-pidpisant/";
			return;
		}
		NameL.Text = "Відповіді на типові запитання:";
		UrlT.Text = "https://www.webchek.com.ua/helpie_faq/";
	}

	private void CopyURLb_Click(object sender, EventArgs e)
	{
		try
		{
			Clipboard.SetText(UrlT.Text);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	private void OpenURLb_Click(object sender, EventArgs e)
	{
		try
		{
			Process.Start(UrlT.Text);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	private void setting_Click(object sender, EventArgs e)
	{
		try
		{
			Process.Start(All.f.FileName, "Notepad.exe");
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			Interaction.MsgBox("Такого файлу немає!", MsgBoxStyle.Information, "setting.ini");
			ProjectData.ClearProjectError();
		}
	}

	private void osp_Click(object sender, EventArgs e)
	{
		try
		{
			Process.Start(ospus.FileName, "Notepad.exe");
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			Interaction.MsgBox("Такого файлу немає!", MsgBoxStyle.Information, "setting.ini");
			ProjectData.ClearProjectError();
		}
	}

	private void SetGetB_Click(object sender, EventArgs e)
	{
		FormHelp formHelp = new FormHelp();
		formHelp.ShowDialog();
		formHelp.Dispose();
	}
}
